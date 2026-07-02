# observatory-store

Backend-agnostic store traits for [`observatory-core`] atoms and
[`observatory-observations`] observations.

The traits speak only domain types (`Atom` / `AtomId` / `Observation` /
`ObservationId` / `Kind`) and a single [`StoreError`] — never a backend's
concrete types. A concrete implementation (e.g. [`observatory-lance`])
supplies the trait; the rest of the system programs against the trait.

## Why a trait crate

Holding the trait lets downstream code depend on a stable contract without
coupling to a backend, and lets a test double stand in for the real store
without dragging the backend's dependency graph in. The Lance implementation
lives in [`observatory-lance`] — depend on that crate to get a real store;
depend on this crate to write code that is backend-agnostic.

## Write methods are streaming-first

`put_atoms` and `put_observations` take a [`Stream`](futures::stream::Stream)
of items, not a `&[T]`. A parser over a large XLIFF, a network feed, or any
incremental producer can feed the store without materializing the whole
batch in memory. Callers with an in-memory slice use [`futures::stream::iter`]:

```rust
use futures::stream;
use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_store::AtomStore;
# use observatory_store::StoreError;

async fn round_trip<S: AtomStore>(store: &mut S) -> Result<(), StoreError> {
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

Chunking policy — how the implementation buffers the stream before committing
— is the implementation's concern, never the trait's. Reads return `Vec<T>`:
the latency budget is generous, and a single materialized result is the
simplest correct shape.

## What's off the trait

Lifecycle (`open`, `create`) and backend-specific maintenance
(`ensure_indexes`, `optimize_indexes`, `compact`, `cleanup_versions`) stay
off the traits: they take backend-specific options and have no domain-level
reading. They live as inherent methods on the concrete implementation — code
that needs them holds the concrete type, not the trait.

## Status

The two traits and a single `StoreError` are defined. The Lance
implementation lives in [`observatory-lance`]. See [`DESIGN.md`](DESIGN.md)
for the full design.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.

[`observatory-core`]: ../observatory-core/
[`observatory-observations`]: ../observatory-observations/
[`observatory-lance`]: ../observatory-lance/
[`StoreError`]: crate::StoreError
