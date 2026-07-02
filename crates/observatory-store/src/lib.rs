//! Backend-agnostic store traits for [`observatory-core`] atoms and
//! [`observatory-observations`] observations.
//!
//! The traits speak only domain types (`Atom` / `AtomId` / `Observation` /
//! `ObservationId` / `Kind`) and a single [`StoreError`] — never a backend's
//! concrete types. A concrete implementation (e.g. [`observatory-lance`])
//! supplies the trait; the rest of the system programs against the trait.
//!
//! [`observatory-lance`]: ../observatory-lance/

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod store;

pub use crate::error::{Result, StoreError};
pub use crate::store::{AtomStore, ObservationStore};

/// Compiles the code examples in the crate README as doc-tests, so they can't
/// drift from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
