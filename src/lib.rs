//! Observatory turns translation segments into content-addressed **atoms**.
//!
//! A translation segment — text that may carry inline formatting — is recorded
//! as an [`Atom`](ir::Atom): a single-language string in which every inline tag
//! is reduced to an opaque *placeholder*. Each atom has a stable, content-derived
//! identity, the [`AtomId`](identity::AtomId) — a SHA-256 over a canonical,
//! normalized serialization of its content and language. Identical content in the
//! same language always yields the same `AtomId`, regardless of how the segment
//! was tagged or how its text happened to be split into runs.
//!
//! The crate is a small pipeline:
//!
//! - [`ir`] — the data model: the [`Atom`](ir::Atom), its content nodes, and the
//!   validated [`LanguageTag`](ir::LanguageTag).
//! - [`normalize`] — configurable rules
//!   ([`NormalizationProfile`](normalize::NormalizationProfile)) for how content
//!   is canonicalized before hashing.
//! - [`identity`] — the content addressing: structural collapse, canonical
//!   serialization, and the [`AtomId`](identity::AtomId).
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
