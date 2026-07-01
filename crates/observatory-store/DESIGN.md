# observatory-store — design

The persistence layer for Observatory: the crate that writes **atoms** and
**observations** to disk and reads them back, faithfully, and does nothing else.

This document is the agreed design. It is written before the code so we can argue
with the shape, not the implementation. Where a decision is settled it says so;
where a decision is deferred it says that too, and why.

## Role, and the line it does not cross

`observatory-store` is a **dumb mechanism**, in the same spirit as
`observatory-core` and `observatory-observations`. It persists what it is given
and returns what it stored, byte for byte. It holds **no semantics**:

- it does not decide which `Kind`s are valid, at which arity, with which payload;
- it does not interpret subject order (direction vs. noise is the kind's meaning);
- it does not normalize atoms (callers fold with `observatory-core::normalize`
  before handing an atom over);
- it does not implement a query language — compound, multi-hop queries are a
  query engine's job (DuckDB, later), not the store's.

Its one earned opinion is **content addressing as an integrity contract**: it
computes an atom's `AtomId` itself rather than trusting a caller-supplied one, so
a row's key is always a true digest of its content. Everything else is mechanism.

Place in the crate DAG:

```
observatory-core  ◄── observatory-observations
       ▲                      ▲
       └────────┬─────────────┘
                │
        observatory-store   (this crate)
```

It depends on both upstream crates by path, persists their types, and adds no
model of its own.

## Substrate and rationale

- **Lance is the store.** Chosen over Parquet for ~100× faster random access,
  which point-lookups by id need. A Lance dataset is an immutable, versioned
  directory (`*.lance`) — every write produces a new version, like a git commit,
  and old versions remain readable until cleaned up. This versioning is not
  incidental; it shapes the handle model below.
- **DuckDB is a query engine over Lance, not a second store** — and it is
  **deferred**. Point lookups and array-membership go through native Lance +
  scalar indexes; compound/1–2-hop queries will go to DuckDB later, reading Lance
  either through the new DuckDB `lance` extension or, conservatively, via an
  Arrow handoff from Rust. Nothing in increment 1–3 needs it.
- **The latency budget is generous.** The consumer is RAG-style context assembly
  for translation agents; an LLM call dwarfs storage, so 500 ms–2 s is fine. We
  buy simplicity with that budget wherever we can.

**Version constraints (hard):** `lance = 7.0.0`, the `arrow*` crates pinned to
**58.x** (Lance 7.0.0's Arrow major — a mismatch yields opaque trait-bound
errors), `tokio` with a **multi-threaded** runtime (Lance's I/O pool can deadlock
on a current-thread runtime). These are added via `cargo add`, never by hand.

## On-disk layout

The two tables are **two independent stores over two independent datasets**, not
one combined store — see the next section for why. By convention they live as
siblings under a shared root:

```
<root>/
  atoms.lance/          # AtomStore's dataset
  observations.lance/   # ObservationStore's dataset   (increment 2)
```

The sibling-under-root layout is a _convention_ (a caller opens both under one
root), not a coupling enforced by the types. The two tables never join at the
storage layer; joins are the query engine's job.

## Two stores, and the handle model

The crate exposes **two independent store types** — `AtomStore` and
`ObservationStore` — each wrapping exactly one Lance dataset. They are _not_
bundled into one struct, for two reasons:

- **No combined transaction to protect.** Lance versions each dataset
  independently; there are no cross-dataset transactions. "Write atoms, then write
  observations" is two separate commits regardless. A combined store could enforce
  no invariant that two structs cannot, so the coupling buys nothing.
- **Borrow independence.** Both designs hold two `Arc<Dataset>`; the question is
  one struct or two. In one combined `Store`, a writer's `&mut self` borrows the
  _whole_ struct, so you could not read observations while a write to atoms is
  borrowed. As two structs, `&mut atom_store` and `&observation_store` are
  independent borrows — concurrent atom-write + observation-read just works, which
  the async, concurrent workload wants.

Plus single responsibility: each store has one schema and one op set, a read-only
consumer needn't open the write-heavy table, and each is testable alone. Shared
mechanism (open-a-dataset, the version swap below, scan-to-batches) lives in
private helpers so nothing is duplicated. A thin facade that opens both under one
root could sit on top later if a caller wants one-call open — YAGNI now.

**The handle model (identical in both stores).** A Lance dataset is an immutable
versioned snapshot; a write returns a _new version's handle_ rather than mutating
in place. Lance hands these out as `Arc<Dataset>` (a cheap, shareable,
reference-counted handle) because one dataset may be read by many concurrent
tasks. So each store:

- holds a single `Arc<Dataset>`;
- **readers borrow `&self`** (`get_atom`, `observations_about`, …) and can run
  concurrently;
- **writers borrow `&mut self`** (`put_atoms`, `put_observations`): the write runs
  `merge_insert`/append, gets back the _new_ version's `Arc<Dataset>`, and **swaps
  it into the field** — reassigning needs the exclusive `&mut` borrow. Holding
  `Arc<Dataset>` (not a bare `Dataset`) is deliberate: `merge_insert` both takes
  and returns an `Arc<Dataset>`, so the field type matches its currency and no
  wrap/unwrap dance is needed.

A future "many readers share one store across threads" need could move writers to
`&self` via interior mutability (a lock around the handle). We are **not** doing
that now — `&mut self` is the simplest correct choice and we add the lock only if
a real caller needs it.

All store methods are `async fn`. The crate is a library: it **starts no
runtime**. Callers (a binary's `#[tokio::main]`, or our `#[tokio::test(flavor =
"multi_thread")]`) provide the multi-threaded runtime the futures are awaited on.

## Write granularity (batch-first)

The workload is bursty — translating one XLIFF yields many atoms and many
observations at once — so the stores are **batch-first by construction**, and this
is a design commitment from day one even though the tuning is deferred:

- The write APIs take **slices** (`put_atoms(&[Atom])`,
  `put_observations(&[Observation])`), and **one call is one operation producing
  one new dataset version**: the whole slice is encoded into a single Arrow
  `RecordBatch` and handed to Lance as one `merge_insert` (atoms) or one append
  (observations). A call is never a per-item loop of writes.
- **Callers batch at the call site.** Accumulate a whole logical unit — every atom
  and observation from one translated XLIFF — then make one `put_*` call. The
  store does not coalesce across calls; one call, one commit.
- This is correctness-adjacent, not just speed: Lance creates a **new fragment per
  write**, so many tiny writes fragment the dataset and slow every later scan.
  Batch-first keeps fragment count proportional to logical write units, not row
  count.
- Within one input slice, **duplicate `atom_id`s are harmless** (content
  addressing makes duplicates byte-identical); the encoder may dedup-by-id within
  a batch before `merge_insert` to keep the merge clean. Noted for implementation.
- **Deferred (increment 3):** chunking a very large logical write into several
  `RecordBatch`es and tuning `WriteParams` (`max_rows_per_file` /
  `max_rows_per_group`) — still one commit; and compaction, which reclaims
  fragmentation after the fact. A streaming/builder API that feeds a
  `RecordBatchReader` incrementally (for imports too large to hold in memory) is a
  possible future ergonomic — the slice API + caller-side accumulation covers the
  XLIFF workload for now.

## Atoms table

Schema:

| column       | Arrow type                                  | notes                                                                                                              |
| ------------ | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `atom_id`    | `FixedSizeBinary(32)`                       | the SHA-256 _matching_ key; lossy (excludes placeholder markup), so **not unique**. BTREE (deferred).              |
| `row_digest` | `FixedSizeBinary(32)`                       | the SHA-256 _exact_ key over the full atom incl. markup; the dedup/upsert key. Internal — never exposed.           |
| `language`   | `Utf8`                                      | the BCP-47 tag, verbatim. Lance auto-dictionary-encodes low-cardinality values; no Arrow `Dictionary` type needed. |
| `content`    | `List<Struct<node_kind: Utf8, data: Utf8>>` | the atom's content nodes, in order.                                                                                |

**The `content` shape, and why it is what it is.** A stored atom must round-trip
to the exact `Vec<ContentNode>`, because the variant matters: re-deriving the
`AtomId` treats a `Placeholder` as a bare tag (its markup is excluded from
identity) and `Text` as its full bytes. So a discriminator is unavoidable; the
only question was its shape.

The honest representation of the `Text | Placeholder` enum is an Arrow `Union`,
and that was the preference — but **a verified spike proved Lance 7.0.0 hard-
rejects `DataType::Union`** at schema conversion (`lance-core` returns
`Err("Unsupported data type")` before any bytes are written; nesting it inside a
`List` cannot help). So the discriminator is carried as a **named tag**:
`node_kind ∈ {"text", "placeholder"}` alongside the node's `data`. This keeps the
Arrow schema itself the faithful serialization (no bespoke blob format), reads
itself (no "what does `true` mean" boolean blindness), and DuckDB reads it
natively. Encode maps the enum to the tag; decode maps the tag back, and an
unrecognized tag is a `StoreError` (a foreign/corrupt row), never a panic.

**Two keys, two jobs.** The store derives _both_ keys itself from the atom's
content (`put_atoms` takes plain `Atom`s; callers never supply a key, so a row's
keys cannot disagree with its content):

- **`atom_id`** — `observatory_core::identity::id_from_atom`, the _matching_ key. It
  deliberately excludes placeholder markup (a `Placeholder` hashes as a bare tag),
  so two atoms differing only in markup — `click <ph/>here` vs `click <bpt/>here` —
  share an `atom_id`. It is therefore **lossy and non-unique**, and it is the key
  observations reference and that TM matching will use.
- **`row_digest`** — a SHA-256 over a length-framed serialization of the language
  and the _full_ content, markup included. It is **exact and injective**, and it is
  the `merge_insert` dedup key. Framing (a length prefix per field, a variant tag
  per node) is load-bearing: a plain concatenation like `Atom::reconstruct()` is
  _not_ injective (adjacent placeholders `["ab","c"]` and `["a","bc"]` both flatten
  to `"abc"`), which would silently collapse distinct atoms.

Consequences: `put_atoms` dedups on `row_digest` (`WhenMatched::DoNothing`), so a
byte-identical re-put is a true no-op while genuine markup variants both persist;
`row_digest` is an implementation detail (callers never see or query it, and
`decode` ignores it). And because `atom_id` is non-unique, lookup returns **all**
matches — `get_atoms_by_id(id) -> Vec<Atom>` — the store never picks a winner among
variants (that is the app's call, via observations).

## Observations table (increment 2 — DONE)

The six-column envelope, matching `observatory-observations` exactly:

| column           | Arrow type                      | notes                                                                                       |
| ---------------- | ------------------------------- | ------------------------------------------------------------------------------------------- |
| `observation_id` | `FixedSizeBinary(32)`           | the content-addressed SHA-256, derived by the store via `id_from_observation`.              |
| `kind`           | `Utf8`                          | the kind label, verbatim. BITMAP (deferred).                                                |
| `subjects`       | `List<FixedSizeBinary(32)>`     | the keyed atoms, in order. LABEL_LIST (deferred).                                           |
| `recorded_at`    | `Timestamp(Microsecond, "UTC")` | transaction time. BTREE (deferred).                                                         |
| `effective_at`   | `Timestamp(Microsecond, "UTC")` | valid time. BTREE (deferred).                                                               |
| `payload`        | `Utf8`                          | the `serde_json::Value`, serialized to a JSON string; DuckDB parses it with JSON functions. |

**Content-addressed identity, not minted.** `observation_id` is a 32-byte SHA-256
over the observation's canonical serialization (kind, subjects in order, both
timestamps, and the canonical JSON of the payload), derived by the store itself
— the same integrity contract as `atom_id`/`row_digest` on the atoms side. The
id lives *off* the `Observation` struct; callers call
`id_from_observation(&obs)` to learn it, as they call `id_from_atom(&atom)`.

**One key, not two.** Unlike the atoms table (which carries `atom_id` for
matching and `row_digest` for exact dedup), the observations table has only
`observation_id` — it is both the matching key and the exact dedup key, because
observations have no analog to the atom's placeholder-markup-exclusion. There is
no "lossy matching" vs "exact identity" split.

**Canonical JSON.** The payload is a `serde_json::Value`, and JSON object key
order is not semantically significant. To keep the id reproducible across
serializers and `serde_json` feature flags, the payload is fed through a small
canonicalizer before hashing: object keys are sorted alphabetically at every
depth, arrays are left in order, and the result is emitted compactly. The
canonicalizer sorts keys itself rather than relying on `serde_json`'s internal
`BTreeMap`, so the output is pinned by the code, not by a feature flag.

Writes use **`merge_insert` on `observation_id`** with `WhenMatched::DoNothing`:
a byte-identical re-put is a true no-op, while a genuinely distinct observation
is inserted. This mirrors `put_atoms`'s `row_digest` dedup exactly — the
content-addressed id makes dedup-on-write correctness-preserving, and the
"append-only, never upsert" stance of the earlier design was predicated on the
now-abandoned minted-id model. Two recordings that differ only in subject order
are distinct observations (subject order feeds the hash), exactly as the
observations crate intends.

**Timestamp encoding:** `SystemTime` ↔ `Timestamp(µs, UTC)` is a signed `i64`
micros count, with `duration_since(UNIX_EPOCH)`'s `Err` variant yielding the
negative magnitude for pre-epoch backfilled history. Round-trips exactly through
`system_time_to_micros` / `micros_to_system_time` in `observatory-observations`.

**Symmetric-kind canonicalization** is **deferred**. The design intent is to
sort subjects for symmetric kinds on write so a derived TM does not double-count
`[fr,en]` and `[en,fr]`. But symmetry is a _per-kind_ property that lives in a
kind registry that does not exist yet. Until it does, the store records subjects
**faithfully, as given**, and canonicalization is deferred — consistent with "the
store collapses nothing the model didn't ask it to."

## API surface

Async signatures (illustrative — the design, not the implementation):

```rust
impl AtomStore {
    // lifecycle — two primitives, no open_or_create (a create_if_missing flag or a
    // convenience combinator could be added later); URIs are &str, not &Path
    async fn open(uri: &str) -> Result<AtomStore>;    // errors if absent
    async fn create(uri: &str) -> Result<AtomStore>;  // empty dataset; errors if present

    // atoms — increment 1 (implemented)
    async fn put_atoms(&mut self, atoms: &[Atom]) -> Result<()>;       // dedup-by-row_digest upsert, one commit
    async fn get_atoms_by_id(&self, id: AtomId) -> Result<Vec<Atom>>;  // all matches for the non-unique id

    // maintenance — increment 3
    async fn ensure_indexes(&mut self) -> Result<()>;                      // BTREE on atom_id
    async fn compact(&mut self) -> Result<()>;
    async fn cleanup_versions(&mut self) -> Result<()>;
}

impl ObservationStore {
    // lifecycle (same shape as AtomStore)
    async fn open(uri: &str) -> Result<ObservationStore>;
    async fn create(uri: &str) -> Result<ObservationStore>;

    // observations — increment 2 (implemented)
    async fn put_observations(&mut self, observations: &[Observation]) -> Result<()>;  // merge_insert on observation_id, one commit
    async fn get_observation_by_id(&self, id: ObservationId) -> Result<Option<Observation>>;  // point lookup
    async fn get_observations_of_kind(&self, kind: &Kind) -> Result<Vec<Observation>>;        // scan filter on kind

    // deferred to increment 3 (needs LABEL_LIST index)
    // async fn observations_about(&self, atom: AtomId) -> Result<Vec<Observation>>;

    // maintenance — increment 3
    async fn ensure_indexes(&mut self) -> Result<()>;                      // BITMAP on kind, LABEL_LIST on subjects
    async fn compact(&mut self) -> Result<()>;
    async fn cleanup_versions(&mut self) -> Result<()>;
}
```

Each store owns the maintenance of its own dataset, so `ensure_indexes` /
`compact` / `cleanup_versions` appear on both.

- `put_atoms` is **idempotent**: `merge_insert` keyed on `row_digest` (the exact
  key) with `WhenMatched::DoNothing` / `WhenNotMatched::InsertAll`. A byte-identical
  re-put is a no-op; markup variants (same `atom_id`, different `row_digest`) both
  persist. Empty input is a no-op that writes no version. It returns `()` — Lance's
  `MergeStats` is deliberately not surfaced (exposing it would leak the backend; a
  backend-agnostic `PutStats` could be added if a caller ever needs the counts).
- `get_atoms_by_id` returns **every** atom under the id (markup variants included),
  unranked; an unknown id yields an empty vec. `verify_atom` was considered and
  **dropped** (YAGNI — fighting unobserved corruption atop content-addressing and
  Lance's own checksums).

## Error strategy

A single `StoreError` enum and `type Result<T> = std::result::Result<T,
StoreError>`. It wraps the foreign errors (`lance::Error`, the Arrow errors, and
later `serde_json::Error`) and adds the store's own variants — e.g. a row whose
`language` is not a valid `LanguageTag`, or a `content` row with an unknown
`node_kind`. **No `unwrap`/`expect` in library code**; every fallible boundary
returns a `StoreError`.

## Indexes (increment 3)

Built via the `DatasetIndexExt::create_index` extension trait once the tables hold
data:

- `atom_id` → **BTREE** (point lookups, joins).
- `kind` → **BITMAP** (low-cardinality equality filters).
- `subjects` → **LABEL_LIST** (array membership: "every observation touching atom
  X").

Indexes are deferred because `put`/`get` are correct without them — just slower —
and an index over an empty table is pointless. Note for later: new rows written
after an index is built are unindexed until rebuilt (`create_index(replace:
true)`), and compaction remaps row ids (rebuild or use remap options).

## DuckDB / query path (increment 4, deferred)

Compound and multi-hop queries go to DuckDB over Lance. The store's contribution
is minimal: expose the dataset path (and/or an Arrow scanner) so a query engine
can read it. Whether that is the DuckDB `lance` extension or a Rust→Arrow→DuckDB
handoff is decided when we get there; both work, the extension is newer.

## Build plan

Built in small, reviewable increments, one file at a time, pausing at each.
Intermediate commits need not compile (history is cleaned with `jj` at the end).

1. **Atoms round-trip (DONE).** `error.rs` → `schema.rs` (atoms, incl. the internal
   `row_digest` column) → `encode.rs`/`decode.rs` → `atom_store.rs` (`AtomStore`:
   `open`, `create`, `put_atoms`, `get_atoms_by_id`; `verify_atom` dropped). Covered
   by unit tests on the conversion and integration tests that round-trip through a
   real on-disk dataset in a tempdir.
2. **Observations (DONE).** `ObservationStore` in `observation_store.rs`: schema +
   encode/decode (the `List<FixedSizeBinary>` subjects, the two timestamps, the
   JSON payload) + `put_observations` (content-addressed `merge_insert`) +
   `get_observation_by_id` + `get_observations_of_kind`. Content-addressed
   identity (`observation_id` is a SHA-256 derived by the store, mirroring
   `atom_id`/`row_digest`), with a canonical JSON payload serializer and signed
   `i64` micros timestamp encoding (pre-epoch supported). Covered by unit tests
   on the conversion, integration tests through a real on-disk dataset, and a
   proptest round-trip property. `observations_about` deferred to increment 3
   (needs the LABEL_LIST index it was designed for; spike on Lance's array-
   contains filter syntax deferred rather than committing to a fallback DuckDB
   or the index will obsolete).
3. **Indexes + maintenance.** `ensure_indexes`, `compact`, `cleanup_versions`.
4. **Query path.** Expose the dataset path / Arrow scanner for DuckDB.

## Deferred decisions, collected

- Scalar indexes — increment 3.
- `observations_about` (array-membership on `subjects`) — increment 3, with the
  LABEL_LIST index.
- Symmetric-kind subject canonicalization (needs a kind registry that does not yet
  exist) — deferred; the store records subjects faithfully, as given.
- Interior-mutability shared store (writers on `&self`) — only if a caller needs
  it.
- DuckDB read path and which integration (extension vs. Arrow handoff) —
  increment 4.
- **Backend split into `observatory-arrow` + `observatory-lance`.** The
  domain↔Arrow mapping (`schema` / `encode` / `decode`) depends only on `arrow`;
  Lance enters only at the store types. That seam is kept as an internal _module_
  boundary now — the arrow-mapping modules never import `lance` — so extracting two
  crates later is a near-mechanical refactor, done only if a real second backend
  (e.g. `observatory-parquet`) is ever wanted. **Lance is the first-class citizen:**
  the shared schema deliberately targets what Lance can store (the `Union`
  rejection is precisely this), so any future backend must support at least that.
  The store trait(s) are **derived, not predicted**: we build the concrete Lance
  store fully, then distill the minimal trait from it — and _that_ distillation is
  when the split happens. Likely trigger: wanting a test-double/mock store (probably
  before a second real backend), or the shared shape becoming obvious once
  `ObservationStore` also exists. Discipline that keeps the split mechanical: `lance`
  stays confined to the store modules, and every intended-interface signature speaks
  only domain types (`Atom` / `AtomId` / `StoreError`), never Lance types — Lance may
  surface only in impl-specific extras that live _outside_ the trait (e.g. a
  stats-returning `put_atoms_with_stats`).
