//! The two store traits — the backend-agnostic contracts a concrete store
//! implements.
//!
//! Both traits are **write-streaming-first**: the write methods take a
//! [`Stream`](futures::stream::Stream) of items rather than a `&[T]`, so a
//! caller can feed atoms or observations produced incrementally (a parser
//! over a large XLIFF, a network feed) without materializing the whole batch
//! in memory. A concrete implementation is free to buffer the stream into
//! chunks of a size it picks — chunking policy is the implementation's
//! concern, never the trait's. Callers with an in-memory batch use
//! [`futures::stream::iter`].
//!
//! Reads return `Vec<T>`: the latency budget is generous (storage dwarfs the
//! LLM calls downstream), and a single materialized result is the simplest
//! correct shape. A streaming read API can be added alongside the vec-returning
//! methods later if a real need emerges.
//!
//! Lifecycle (`open`, `create`) and backend-specific maintenance (`ensure_indexes`,
//! `optimize_indexes`, `compact`, `cleanup_versions`) stay **off the traits**:
//! they take backend-specific options and have no domain-level reading, so
//! they live as inherent methods on the concrete implementation. Code that
//! needs them holds the concrete type, not the trait.

use async_trait::async_trait;
use futures::stream::Stream;

use observatory_core::identity::AtomId;
use observatory_core::ir::Atom;
use observatory_observations::identity::ObservationId;
use observatory_observations::{Kind, Observation};

use crate::error::Result;

/// Backend-agnostic persistence for atoms.
///
/// The trait speaks only domain types; the concrete implementation (e.g.
/// [`observatory_lance::LanceAtomStore`]) supplies the Lance-backed behavior
/// and owns Lance-specific lifecycle and maintenance methods alongside.
///
/// [`observatory_lance::LanceAtomStore`]: ../observatory_lance/struct.LanceAtomStore.html
#[async_trait]
pub trait AtomStore: Send {
    /// Stores `atoms`, deriving each key itself and upserting by exact
    /// identity. An atom already present byte-for-byte is left untouched, so
    /// the call is idempotent, while a genuine variant is inserted.
    ///
    /// The stream is consumed in implementation-defined chunks; one call is
    /// one logical commit (the implementation decides how many physical
    /// writes that becomes). An empty stream is a no-op that writes nothing.
    async fn put_atoms(
        &mut self,
        atoms: impl Stream<Item = Atom> + Unpin + Send + 'static,
    ) -> Result<()>;

    /// Returns every atom stored under `id`. Because `atom_id` is the lossy
    /// matching key (it excludes placeholder markup), this can be more than
    /// one atom — the markup variants that share an id — and the store returns
    /// them all, unranked; choosing among them is the caller's concern.
    /// An unknown id yields an empty vector, not an error.
    async fn get_atoms_by_id(&self, id: AtomId) -> Result<Vec<Atom>>;
}

/// Backend-agnostic persistence for observations.
///
/// The trait speaks only domain types; the concrete implementation (e.g.
/// [`observatory_lance::LanceObservationStore`]) supplies the Lance-backed
/// behavior and owns Lance-specific lifecycle and maintenance methods
/// alongside.
///
/// [`observatory_lance::LanceObservationStore`]: ../observatory_lance/struct.LanceObservationStore.html
#[async_trait]
pub trait ObservationStore: Send {
    /// Stores `observations`, deriving each id itself and upserting by
    /// content-addressed identity. An observation already present
    /// byte-for-byte is left untouched, so the call is idempotent, while a
    /// genuinely distinct observation is inserted.
    ///
    /// The stream is consumed in implementation-defined chunks; one call is
    /// one logical commit. An empty stream is a no-op that writes nothing.
    async fn put_observations(
        &mut self,
        observations: impl Stream<Item = Observation> + Unpin + Send + 'static,
    ) -> Result<()>;

    /// Returns the observation whose content-addressed id is `id`, or `None`
    /// if no such observation is stored. The id is unique (it is the exact
    /// identity), so at most one match exists.
    async fn get_observation_by_id(
        &self,
        id: ObservationId,
    ) -> Result<Option<Observation>>;

    /// Returns every observation whose `kind` matches. The result is unranked;
    /// choosing among them is the caller's concern. An unknown kind yields an
    /// empty vector, not an error.
    async fn get_observations_of_kind(&self, kind: &Kind) -> Result<Vec<Observation>>;

    /// Returns every observation whose `subjects` list contains `atom`. The
    /// result is unranked; choosing among them is the caller's concern. An
    /// atom observed by nothing yields an empty vector, not an error.
    async fn get_observations_by_subject(&self, atom: AtomId) -> Result<Vec<Observation>>;
}
