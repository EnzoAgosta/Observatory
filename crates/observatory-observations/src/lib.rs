//! Append-only **observations** over [`observatory_core`] atoms.
//!
//! An atom records only *what a string is*. Everything *about* a string, or
//! *between* strings, is an [`Observation`]: an append-only fact keyed to one or
//! more [`AtomId`](observatory_core::identity::AtomId)s. There are two shapes,
//! distinguished only by how many atoms they touch:
//!
//! - a **property** — a fact about a single atom: it has been reviewed,
//!   blacklisted, assigned a domain, used in a campaign.
//! - a **relationship** — a fact among atoms: two are translations of each other,
//!   one is context for another, several are interchangeable.
//!
//! The two are built with faithful, intent-named constructors —
//! [`Observation::property`] and [`Observation::relationship`] — which record
//! exactly the subjects they are given, the way
//! [`Atom::new`](observatory_core::ir::Atom::new) records its content nodes. The
//! names guide the reader rather than enforce a split: both build the same shape,
//! so nothing prevents a one-subject relationship, and sharper constructors or
//! deeper validation are conveniences for later, not policy the primitive imposes
//! now.
//!
//! ## Identity is content-derived
//!
//! An [`Observation`]'s identity is not minted — it is *derived* from its content
//! by [`id_from_observation`], a SHA-256 over a canonical serialization of every
//! field (kind, subjects in order, both timestamps, and the payload). The same
//! observation always yields the same id, so byte-identical observations share one
//! and the storage layer dedups them naturally — the same integrity contract
//! [`observatory_core`] applies to atoms. Both timestamps feed the hash, so a
//! re-asserted fact at a different time is a distinct id, exactly as the
//! append-only model intends.
//!
//! ## Structure, not semantics
//!
//! This crate is a dumb primitive in the same spirit as [`observatory_core`]:
//! construction is faithful and judges almost nothing. A [`Kind`] is an open,
//! user-definable label, checked only for being non-empty; which kind is
//! "allowed" at which arity, and what its payload must contain, is *semantics*
//! that lives a layer up. A registry of such rules can be earned later, when
//! enough kinds exist to make centralizing them worthwhile; until then, a kind is
//! a label the caller is trusted to use consistently.
//!
//! Subject order is *preserved* exactly as given, and it feeds the id, so two
//! recordings that differ only in subject order are distinct observations here.
//! Whether that order carries a *direction* or is noise is the kind's semantics:
//! a translation is a **symmetric** equivalence — `en-US` ⇄ `fr-FR`, neither side
//! privileged, so the directional source→target view a classic TM bakes into
//! storage is instead derived at export time — while a directed kind reads its
//! subject order. The crate enforces none of this: canonicalizing a symmetric kind
//! (sorting its subjects so the two orders collapse to one id) is the caller's
//! job, applied before calling [`id_from_observation`], just as
//! [`observatory_core`] records atoms faithfully and leaves normalization to the
//! caller.
//!
//! ## Time is bitemporal
//!
//! Every observation carries two timestamps. [`recorded_at`](Observation::recorded_at)
//! is when the fact was written down; [`effective_at`](Observation::effective_at)
//! is when it became true in the world. They differ when history is backfilled —
//! recording a 2019 approval in 2026 — and are equal otherwise, which is why
//! `effective_at` defaults to `recorded_at` when a constructor is passed `None`.
//! As with the rest of the system, the clock is the caller's: timestamps are
//! arguments, never read from an ambient source here.
//!
//! ## The payload is a kitchen sink
//!
//! Everything kind-specific — provenance (who or what asserted it), scores,
//! reasons, campaign ids — lives in the [`payload`](Observation::payload), an
//! opaque [`serde_json::Value`]. The envelope above it (kind, subjects, the two
//! timestamps) is the only thing every observation shares; the id is a derived
//! projection over the whole, not a field. Storage and query layers project the
//! payload into typed columns or views as concrete queries earn it.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod identity;
mod kind;
mod observation;

pub use identity::{ObservationId, id_from_observation};
pub use kind::{Kind, KindError};
pub use observation::Observation;

/// Compiles the code example in the crate README as a doc-test, so it can't
/// drift from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
