# observatory-store

Lance-backed persistence for [`observatory-core`] atoms and
[`observatory-observations`] observations.

The store is a **dumb mechanism**: it writes what it is given to disk and returns
what it stored, faithfully, and holds no semantics of its own. It does not decide
which `Kind`s are valid, interpret subject order, normalize atoms, or run a query
language — those are the calling application's concerns. Its one earned opinion is
**content addressing as an integrity contract**: it derives an atom's keys itself
rather than trusting a caller-supplied one, so a row's keys can never disagree
with its content. The full design lives in [`DESIGN.md`](DESIGN.md).

## Two stores, one dataset each

Persistence is split into two independent types — `AtomStore` and (planned)
`ObservationStore` — each wrapping exactly one Lance dataset, rather than one
combined store. Lance has no cross-dataset transactions, so a combined store would
enforce no invariant two types cannot; and two types give **borrow independence**
— writing atoms (`&mut atom_store`) while reading observations (`&observation_store`)
just works. By convention they live as siblings under a shared root
(`<root>/atoms.lance`, `<root>/observations.lance`), but nothing in the types
couples them.

## Two keys, two jobs

A stored atom carries two content-derived keys, because `atom_id` alone is not a
faithful row key:

- **`atom_id`** — the *matching* key from `observatory-core`. It deliberately
  excludes placeholder markup, so `click <ph/>here` and `click <bpt/>here` share an
  `atom_id`. It is therefore **lossy and non-unique** — the key observations
  reference and that translation-memory matching uses.
- **`row_digest`** *(internal)* — an *exact* SHA-256 over the full atom, markup
  included, using a length-framed (injective) serialization. It is the dedup key
  and never leaves the store.

So a byte-identical re-`put` is a true no-op, while two atoms that differ only in
markup are both kept — and because `atom_id` is non-unique, **lookup returns every
match**: `get_atoms_by_id(id) -> Vec<Atom>`. The store never picks a winner among
variants; that is the application's call, via observations.

## The handle model

A Lance dataset is an immutable, versioned snapshot — every write produces a new
version, like a git commit — so each store holds an `Arc<Dataset>` handle to the
current version. Readers borrow `&self` and share the snapshot; a writer borrows
`&mut self`, runs the write, gets back the new version's handle, and swaps it into
place. Writes are **batch-first**: one `put` call encodes the whole slice into a
single Arrow `RecordBatch` and commits once, so fragment count tracks logical
writes, not row count.

All methods are `async`. The crate is a library and **starts no runtime**; callers
provide a **multi-threaded** Tokio runtime (a current-thread runtime can deadlock
Lance's I/O pool).

## API (`AtomStore`)

```text
open(uri)              -> AtomStore     // errors if absent
create(uri)            -> AtomStore     // empty dataset; errors if present
put_atoms(&[Atom])     -> ()            // dedup-on-write upsert, one commit
get_atoms_by_id(AtomId) -> Vec<Atom>    // every atom filed under the id
ensure_indexes()       -> ()            // BTREE on atom_id (idempotent)
optimize_indexes(&OptimizeOptions) -> () // refresh index to cover new writes
compact(&CompactionOptions) -> ()       // defragment small fragments
cleanup_versions(retain: usize) -> RemovalStats // GC old versions, keep last N
```

## API (`ObservationStore`)

```text
open(uri)                          -> ObservationStore     // errors if absent
create(uri)                        -> ObservationStore     // empty dataset; errors if present
put_observations(&[Observation])   -> ()                   // dedup-on-write upsert, one commit
get_observation_by_id(ObservationId) -> Option<Observation> // point lookup (id is unique)
get_observations_of_kind(&Kind)    -> Vec<Observation>     // all observations of one kind
observations_about(AtomId)          -> Vec<Observation>     // every observation whose subjects contain the atom
ensure_indexes()       -> ()            // BTREE observation_id + BITMAP kind + LABEL_LIST subjects (idempotent)
optimize_indexes(&OptimizeOptions) -> () // refresh indexes to cover new writes
compact(&CompactionOptions) -> ()       // defragment small fragments
cleanup_versions(retain: usize) -> RemovalStats // GC old versions, keep last N
```

Observations are content-addressed: the store derives each `ObservationId` itself
via `id_from_observation` (a SHA-256 over kind, subjects in order, both
timestamps, and the canonical JSON of the payload), so a row's key can never
disagree with its content. The id lives off the `Observation` struct; callers
call `id_from_observation(&obs)` to learn it, as they call `id_from_atom(&atom)`.

URIs are `&str`, so local paths and object-store URIs (`s3://`, `gs://`, …) are
handled uniformly.

## Example

```rust
use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_store::AtomStore;

async fn round_trip() -> observatory_store::Result<()> {
    // Create a new dataset (use `AtomStore::open` for an existing one).
    let mut store = AtomStore::create("data/atoms.lance").await?;

    let atom = Atom::new(
        LanguageTag::from_string("en-US").unwrap(),
        [ContentNode::text("Hello, "), ContentNode::placeholder("<x/>")],
    );

    // One call is one commit; re-putting a byte-identical atom is a no-op.
    store.put_atoms(&[atom.clone()]).await?;

    // Lookup by id returns every atom filed under it (markup variants included).
    let found = store.get_atoms_by_id(id_from_atom(&atom)).await?;
    assert_eq!(found, vec![atom]);

    Ok(())
}
```

## Status

Atom and observation persistence, with indexes and maintenance, are implemented
and tested against real on-disk datasets:

- **`AtomStore`**: `open`/`create`, dedup-on-write `put_atoms`,
  `get_atoms_by_id`, plus index/maintenance primitives — `ensure_indexes`
  (BTREE on `atom_id`), `optimize_indexes`, `compact`, `cleanup_versions`.
- **`ObservationStore`**: `open`/`create`, content-addressed `put_observations`
  (merge_insert on the derived `ObservationId`), `get_observation_by_id`,
  `get_observations_of_kind`, `observations_about` (array-membership on
  `subjects` via the LABEL_LIST index), plus the same index/maintenance
  primitives — `ensure_indexes` (BTREE/`observation_id`, BITMAP/`kind`,
  LABEL_LIST/`subjects`), `optimize_indexes`, `compact`, `cleanup_versions`.

The store is dumps primitives, not policy: when to compact, how many versions
to keep, whether to refresh indexes after every write or nightly is the
calling application's concern. The store never calls `optimize_indexes`
itself — it exposes the primitive, and Lance serves queries correctly (if
slower) without it.

Still to come: the DuckDB query path over the datasets (compound predicates,
range queries, joins). See [`DESIGN.md`](DESIGN.md) for the full plan.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.

[`observatory-core`]: ../observatory-core/
[`observatory-observations`]: ../observatory-observations/
