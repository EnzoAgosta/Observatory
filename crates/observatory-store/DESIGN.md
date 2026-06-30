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

The sibling-under-root layout is a *convention* (a caller opens both under one
root), not a coupling enforced by the types. The two tables never join at the
storage layer; joins are the query engine's job.

## Two stores, and the handle model

The crate exposes **two independent store types** — `AtomStore` and
`ObservationStore` — each wrapping exactly one Lance dataset. They are *not*
bundled into one struct, for two reasons:

- **No combined transaction to protect.** Lance versions each dataset
  independently; there are no cross-dataset transactions. "Write atoms, then write
  observations" is two separate commits regardless. A combined store could enforce
  no invariant that two structs cannot, so the coupling buys nothing.
- **Borrow independence.** Both designs hold two `Arc<Dataset>`; the question is
  one struct or two. In one combined `Store`, a writer's `&mut self` borrows the
  *whole* struct, so you could not read observations while a write to atoms is
  borrowed. As two structs, `&mut atom_store` and `&observation_store` are
  independent borrows — concurrent atom-write + observation-read just works, which
  the async, concurrent workload wants.

Plus single responsibility: each store has one schema and one op set, a read-only
consumer needn't open the write-heavy table, and each is testable alone. Shared
mechanism (open-a-dataset, the version swap below, scan-to-batches) lives in
private helpers so nothing is duplicated. A thin facade that opens both under one
root could sit on top later if a caller wants one-call open — YAGNI now.

**The handle model (identical in both stores).** A Lance dataset is an immutable
versioned snapshot; a write returns a *new version's handle* rather than mutating
in place. Lance hands these out as `Arc<Dataset>` (a cheap, shareable,
reference-counted handle) because one dataset may be read by many concurrent
tasks. So each store:

- holds a single `Arc<Dataset>`;
- **readers borrow `&self`** (`get_atom`, `observations_about`, …) and can run
  concurrently;
- **writers borrow `&mut self`** (`put_atoms`, `put_observations`): the write runs
  `merge_insert`/append, gets back the *new* version's `Arc<Dataset>`, and **swaps
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

| column     | Arrow type                                  | notes                                |
|------------|---------------------------------------------|--------------------------------------|
| `atom_id`  | `FixedSizeBinary(32)`                       | the SHA-256 digest; the key. BTREE (deferred). |
| `language` | `Utf8`                                      | the BCP-47 tag, verbatim. Lance auto-dictionary-encodes low-cardinality values; no Arrow `Dictionary` type needed. |
| `content`  | `List<Struct<node_kind: Utf8, data: Utf8>>` | the atom's content nodes, in order.  |

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

**Identity is the store's to compute.** `put_atoms` takes plain `Atom`s and
derives each `AtomId` via `observatory_core::identity::id_from_atom`. Callers
never supply the key, so a row's key cannot disagree with its content.

## Observations table (increment 2 — schema agreed, details open)

The six-column envelope, matching `observatory-observations` exactly:

| column           | Arrow type                       | notes                                  |
|------------------|----------------------------------|----------------------------------------|
| `observation_id` | `FixedSizeBinary(16)`            | the minted ULID/UUID, as given.        |
| `kind`           | `Utf8`                           | the kind label, verbatim. BITMAP (deferred). |
| `subjects`       | `List<FixedSizeBinary(32)>`      | the keyed atoms, in order. LABEL_LIST (deferred). |
| `recorded_at`    | `Timestamp(Microsecond, "UTC")`  | transaction time. BTREE (deferred).    |
| `effective_at`   | `Timestamp(Microsecond, "UTC")`  | valid time. BTREE (deferred).          |
| `payload`        | `Utf8`                           | the `serde_json::Value`, serialized to a JSON string; DuckDB parses it with JSON functions. |

Writes are **append-only**: `put_observations` never upserts. Two recordings that
differ only in subject order are distinct events, exactly as the observations
crate intends.

**Open for increment 2 (do not decide now):**
- `SystemTime ↔ Timestamp(µs, UTC)` encoding, including times before the Unix
  epoch (backfilled history → a signed micros count).
- **Symmetric-kind canonicalization.** The design intent is to sort subjects for
  symmetric kinds on write so a derived TM does not double-count `[fr,en]` and
  `[en,fr]`. But symmetry is a *per-kind* property that lives in a kind registry
  that does not exist yet. Until it does, the store records subjects **faithfully,
  as given**, and canonicalization is deferred — consistent with "the store
  collapses nothing the model didn't ask it to."

## API surface

Async signatures (illustrative — the design, not the implementation):

```rust
impl AtomStore {
    // lifecycle
    async fn open_or_create(path: &Path) -> Result<AtomStore>;

    // atoms — increment 1
    async fn put_atoms(&mut self, atoms: &[Atom]) -> Result<()>;            // idempotent upsert-by-id, one commit
    async fn get_atom(&self, id: AtomId) -> Result<Option<Atom>>;
    async fn get_atoms(&self, ids: &[AtomId]) -> Result<Vec<(AtomId, Option<Atom>)>>;
    async fn verify_atom(&self, id: AtomId) -> Result<bool>;               // recompute id == stored key

    // maintenance — increment 3
    async fn ensure_indexes(&mut self) -> Result<()>;                      // BTREE on atom_id
    async fn compact(&mut self) -> Result<()>;
    async fn cleanup_versions(&mut self) -> Result<()>;
}

impl ObservationStore {
    // lifecycle
    async fn open_or_create(path: &Path) -> Result<ObservationStore>;

    // observations — increment 2
    async fn put_observations(&mut self, observations: &[Observation]) -> Result<()>;  // append-only, one commit
    async fn observations_about(&self, atom: AtomId) -> Result<Vec<Observation>>;      // LABEL_LIST
    async fn observations_of_kind(&self, kind: &Kind) -> Result<Vec<Observation>>;     // BITMAP

    // maintenance — increment 3
    async fn ensure_indexes(&mut self) -> Result<()>;                      // BITMAP on kind, LABEL_LIST on subjects
    async fn compact(&mut self) -> Result<()>;
    async fn cleanup_versions(&mut self) -> Result<()>;
}
```

Each store owns the maintenance of its own dataset, so `ensure_indexes` /
`compact` / `cleanup_versions` appear on both.

- `put_atoms` is **idempotent**: `merge_insert` keyed on `atom_id` with
  "update-all when matched, insert-all when not." Re-putting a byte-identical atom
  is a no-op (and content addressing guarantees duplicates *are* byte-identical).
- `verify_atom` is a near-free faithfulness check: read the row, reconstruct the
  `Atom`, recompute its id, compare to the stored key.

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

1. **Atoms round-trip.** `error.rs` → `schema.rs` (atoms) → `encode.rs`/`decode.rs`
   with unit tests on the conversion (empty content, adjacent text runs,
   placeholder-only, every `node_kind`) → `atom_store.rs` (`AtomStore`:
   `open_or_create`, `put_atoms`, `get_atom(s)`, `verify_atom`) with integration
   tests round-tripping through a real on-disk dataset in a tempdir.
2. **Observations.** `ObservationStore` in `observation_store.rs`: schema +
   encode/decode (the `List<FixedSizeBinary>` subjects, the two timestamps, the
   JSON payload) + `put_observations` (append-only) + `observations_about` /
   `observations_of_kind`. Resolve the open timestamp and canonicalization
   questions here.
3. **Indexes + maintenance.** `ensure_indexes`, `compact`, `cleanup_versions`.
4. **Query path.** Expose the dataset path / Arrow scanner for DuckDB.

## Deferred decisions, collected

- Scalar indexes — increment 3.
- Observations timestamp encoding (incl. pre-epoch) — increment 2.
- Symmetric-kind subject canonicalization (needs a kind registry that does not yet
  exist) — increment 2 at the earliest, possibly later.
- Interior-mutability shared store (writers on `&self`) — only if a caller needs
  it.
- DuckDB read path and which integration (extension vs. Arrow handoff) —
  increment 4.
- **Backend split into `observatory-arrow` + `observatory-lance`.** The
  domain↔Arrow mapping (`schema` / `encode` / `decode`) depends only on `arrow`;
  Lance enters only at the store types. That seam is kept as an internal *module*
  boundary now — the arrow-mapping modules never import `lance` — so extracting two
  crates later is a near-mechanical refactor, done only if a real second backend
  (e.g. `observatory-parquet`) is ever wanted. **Lance is the first-class citizen:**
  the shared schema deliberately targets what Lance can store (the `Union`
  rejection is precisely this), so any future backend must support at least that.
