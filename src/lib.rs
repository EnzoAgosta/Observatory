//! Observatory — the normalization core of a translation data layer.
//!
//! Observatory turns translation segments (which carry inline formatting) into
//! content-addressed *atoms*: a canonical, normalized representation whose
//! identity is `SHA-256(normalized content ‖ canonical BCP-47 language tag)`.
//! Inline-tag *structure* (open / close / standalone, plus pairing) is part of
//! the atom; the original dialect-specific tag *payload* is occurrence data that
//! lives outside identity.
//!
//! The reasoning behind every design choice is recorded in `docs/DECISIONS.md`.
//!
//! Scope (D1): the IR, identity, and normalization only. Storage, retrieval,
//! embeddings, and the XLIFF adapter live elsewhere or in later phases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod identity;
pub mod ir;
pub mod normalize;
