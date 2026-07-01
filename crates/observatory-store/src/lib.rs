//! Lance-backed persistence for `observatory-core` atoms and
//! `observatory-observations` observations.
//!
//! This crate is a dumb mechanism: it writes atoms and observations to disk and
//! reads them back faithfully, and holds no semantics of its own. The agreed
//! design lives in `DESIGN.md` alongside this source.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod atom_store;
mod decode;
mod encode;
mod error;
mod observation_store;
mod schema;

pub use crate::atom_store::AtomStore;
pub use crate::decode::{decode_atoms, decode_observations};
pub use crate::encode::{encode_atoms, encode_observations};
pub use crate::error::{Result, StoreError};
pub use crate::observation_store::ObservationStore;

/// Compiles the code examples in the crate README as doc-tests, so they can't
/// drift from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
