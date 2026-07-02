# observatory-lance

Lance-backed implementation of the [`observatory-store`] traits.

Two concrete store types — [`LanceAtomStore`] and [`LanceObservationStore`] —
each wrap exactly one Lance dataset, implement the corresponding trait, and
additionally expose Lance-specific lifecycle (`open` / `create`) and
maintenance (`ensure_indexes`, `optimize_indexes`, `compact`,
`cleanup_versions`) methods as inherent methods. Code that needs those holds
the concrete type; code that is backend-agnostic holds the trait from
[`observatory-store`].

## The handle model

A Lance dataset is an immutable, versioned snapshot — every write produces a
new version, like a git commit — so each store holds an `Arc<Dataset>` handle
to the current version. Readers borrow `&self` and share the snapshot; a writer
borrows `&mut self`, runs the write, gets back the new version's handle, and
swaps it into place. Writes are **batch-first**: one `put_*` call drains its
input stream in chunks of a fixed size, each chunk encoded into one Arrow
`RecordBatch`, and hands all batches to a single Lance `merge_insert` — one
logical commit per call, with memory bounded by the chunk size rather than by
the stream's total length.

All methods are `async`. The crate is a library and **starts no runtime**;
callers provide a **multi-threaded** Tokio runtime (a current-thread runtime
can deadlock Lance's I/O pool).

## Two stores, one dataset each

Persistence is split into two independent types for borrow independence —
writing atoms while reading observations just works. By convention they live
as siblings under a shared root (`<root>/atoms.lance`,
`<root>/observations.lance`), but nothing in the types couples them.

## Two keys, two jobs (atoms)

A stored atom carries two content-derived keys, because `atom_id` alone is not
a faithful row key:

- **`atom_id`** — the *matching* key from `observatory-core`. It deliberately
  excludes placeholder markup, so `click <ph/>here` and `click <bpt/>here`
  share an `atom_id`. It is **lossy and non-unique** — the key observations
  reference and that translation-memory matching uses.
- **`row_digest`** *(internal)* — an *exact* SHA-256 over the full atom, markup
  included, using a length-framed (injective) serialization. It is the dedup
  key and never leaves the store.

So a byte-identical re-`put` is a true no-op, while two atoms that differ only
in markup are both kept — and because `atom_id` is non-unique, **lookup
returns every match**: `get_atoms_by_id(id) -> Vec<Atom>`. The store never
picks a winner among variants; that is the application's call, via
observations.

Observations have a single `observation_id` — both the matching key and the
exact dedup key — because observations have no analog to the atom's
placeholder-markup-exclusion.

## Lance-specific API

```text
LanceAtomStore::open(uri)              -> LanceAtomStore   // errors if absent
LanceAtomStore::create(uri)           -> LanceAtomStore   // empty dataset; errors if present
ensure_indexes()                       -> ()   // BTREE on atom_id (idempotent)
optimize_indexes(&OptimizeOptions)     -> ()   // refresh index to cover new writes
compact(&CompactionOptions)            -> ()   // defragment small fragments
cleanup_versions(retain: usize)        -> RemovalStats // GC old versions, keep last N
```

```text
LanceObservationStore::open(uri)                  -> LanceObservationStore  // errors if absent
LanceObservationStore::create(uri)                -> LanceObservationStore  // empty; errors if present
ensure_indexes()                                   -> ()   // BTREE/observation_id + BITMAP/kind + LABEL_LIST/subjects (idempotent)
optimize_indexes(&OptimizeOptions)                 -> ()   // refresh indexes to cover new writes
compact(&CompactionOptions)                        -> ()   // defragment small fragments
cleanup_versions(retain: usize)                    -> RemovalStats // GC old versions, keep last N
```

URIs are `&str`, so local paths and object-store URIs (`s3://`, `gs://`, …)
are handled uniformly.

## Example

```rust
use futures::stream;
use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_lance::LanceAtomStore;
use observatory_store::AtomStore;

async fn round_trip() -> observatory_store::Result<()> {
    let mut store = LanceAtomStore::create("data/atoms.lance").await?;

    let atom = Atom::new(
        LanguageTag::from_string("en-US").unwrap(),
        [ContentNode::text("Hello, "), ContentNode::placeholder("<x/>")],
    );

    store.put_atoms(stream::iter([atom.clone()])).await?;

    let found = store.get_atoms_by_id(id_from_atom(&atom)).await?;
    assert_eq!(found, vec![atom]);

    Ok(())
}
```

## Status

Atom and observation persistence, with indexes and maintenance, are
implemented and tested against real on-disk datasets. See [`DESIGN.md`] for
the full design and [`observatory-store`] for the trait contracts this crate
implements.

Maintenance primitives (`ensure_indexes` / `optimize_indexes` / `compact` /
`cleanup_versions`) live as inherent methods here, not on the trait — they
take Lance-specific options and have no domain-level reading. When to call
them is the calling application's concern; the store never calls
`optimize_indexes` itself — it exposes the primitive, and Lance serves queries
correctly (if slower) without it.

Still to come: the DuckDB query path over the datasets (compound predicates,
range queries, joins).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.

[`observatory-store`]: ../observatory-store/
[`DESIGN.md`]: DESIGN.md
[`LanceAtomStore`]: crate::LanceAtomStore
[`LanceObservationStore`]: crate::LanceObservationStore
