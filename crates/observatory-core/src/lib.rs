//! Observatory turns translation segments into content-addressed **atoms**.
//!
//! A translation segment — text that may carry inline formatting — is recorded
//! as an [`Atom`](ir::Atom): a single-language string in which every inline tag
//! is reduced to an opaque *placeholder*. Each atom has a content-derived
//! identity, the [`AtomId`](identity::AtomId) — a SHA-256 over a canonical
//! serialization of the atom *exactly as recorded*. The same atom always yields
//! the same `AtomId`; identity is deliberately dumb and applies no normalization
//! of its own, excluding only placeholder *markup* (so the same text tagged two
//! different ways still shares an id).
//!
//! The crate is a small, layered toolkit:
//!
//! - [`ir`] — the data model: the [`Atom`](ir::Atom), its content nodes, and the
//!   validated [`LanguageTag`](ir::LanguageTag). Construction is faithful.
//! - [`normalize`] — explicit, composable primitives (collapse, trim, Unicode
//!   form, language case) for canonicalizing content *before* taking its id. None
//!   are applied automatically; the caller composes the ones it needs.
//! - [`identity`] — the content addressing: the canonical serialization and the
//!   [`AtomId`](identity::AtomId).
//!
//! Canonicalizing "the same" string across different taggings, chunkings, or
//! casings is the caller's job: normalize the atom, then hash it. Keeping the id
//! itself dumb makes it a pure function of the atom and leaves all policy with
//! the caller (decision D29).
//!
//! An atom records only *what a string is*, never how it relates to other
//! strings. Relationships between strings — translations, reviews, and other
//! facts — are expressed separately as observations over atoms, not baked into
//! the atom itself.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod identity;
pub mod ir;
pub mod normalize;

/// Compiles the code examples in the project README as doc-tests, so they can't
/// drift from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
