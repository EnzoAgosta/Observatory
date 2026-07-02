//! Lance-backed implementation of the [`observatory-store`] traits.
//!
//! Two concrete store types — [`LanceAtomStore`] and [`LanceObservationStore`]
//! — each wrap exactly one Lance dataset, implement the corresponding trait,
//! and additionally expose Lance-specific lifecycle (`open` / `create`) and
//! maintenance (`ensure_indexes`, `optimize_indexes`, `compact`,
//! `cleanup_versions`) methods as inherent methods. Code that needs those
//! holds the concrete type; code that needs to be backend-agnostic holds the
//! trait from [`observatory-store`].
//!
//! Lance types surface only in the inherent methods on these concrete types,
//! never on the trait.
//!
//! [`observatory-store`]: ../observatory-store/

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod atom_store;
mod decode;
mod encode;
mod error;
mod observation_store;
mod schema;

pub use crate::atom_store::LanceAtomStore;
pub use crate::decode::{decode_atoms, decode_observations};
pub use crate::encode::{encode_atoms, encode_observations};
pub use crate::observation_store::LanceObservationStore;

/// Compiles the code examples in the crate README as doc-tests, so they can't
/// drift from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
