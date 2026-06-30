//! Lance-backed persistence for `observatory-core` atoms and
//! `observatory-observations` observations.
//!
//! This crate is a dumb mechanism: it writes atoms and observations to disk and
//! reads them back faithfully, and holds no semantics of its own. The agreed
//! design lives in `DESIGN.md` alongside this source.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod schema;
