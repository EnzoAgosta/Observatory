//! XLIFF 1.2 adapter for [`observatory_core`]: tokenizing one inline content
//! fragment into [`ContentNode`](observatory_core::ir::ContentNode)s and back.
//!
//! This crate is the boundary primitive. It does exactly two things:
//!
//! - [`parse_segment`](parse::parse_segment) turns the XML of an assumed-valid,
//!   already-extracted content node (the body of a `<source>` / `<target>`) into a
//!   `Vec<ContentNode>`.
//! - [`emit_segment`](emit::emit_segment) turns those nodes back into XML.
//!
//! It builds no document model, validates no document, traverses no
//! `<file>` / `<trans-unit>` structure, and records no relationships. Deciding
//! *which* node becomes an atom, assembling the
//! [`Atom`](observatory_core::ir::Atom) with the caller's language, normalizing
//! it, and taking an identity are all the consumer's job. The one decision the
//! parser makes per element is what the XLIFF 1.2 spec declares its content to be:
//! *native code* (`<bpt>`, `<ept>`, `<ph>`, `<it>`) becomes one opaque
//! placeholder; *translatable text* (`<g>`, `<mrk>`) keeps its inner text with the
//! tags recorded as placeholders; empty inline elements (`<x/>`, `<bx/>`, `<ex/>`)
//! are a single placeholder. Placeholder markup is preserved as the raw bytes of
//! the input and is never interpreted.
//!
//! Text entities and CDATA are handled per the [`EntityMode`] — logical (Unicode,
//! the default) or verbatim — and the same mode must be used for both halves of a
//! round-trip.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod emit;
mod error;
pub mod parse;

pub use error::ParseError;

/// How text entity references are handled when crossing the boundary.
///
/// The same mode must be used for both halves of a round-trip: it is not recorded
/// on the produced nodes, so passing the same `EntityMode` to
/// [`parse_segment`](parse::parse_segment) and
/// [`emit_segment`](emit::emit_segment) is what keeps them symmetric. Placeholder
/// markup is always raw and is unaffected by this choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityMode {
    /// Text is decoded to its Unicode form on parse and re-escaped on emit, so
    /// fragments differing only in escaping share an identity. The default.
    Logical,
    /// Text is kept exactly as written, entities and all — a byte-identical
    /// round-trip, at the cost of identity hashing the escaped form.
    Verbatim,
}

/// Compiles the code example in the crate README as a doc-test, so it can't drift
/// from the real API. Exists only during doc-test collection.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDocTests;
