//! XLIFF 1.2 adapter for [`observatory_core`]: a stateless codec between an XLIFF
//! 1.2 content fragment and an [`Atom`](observatory_core::ir::Atom).
//!
//! This crate is the boundary primitive. It does exactly two things:
//!
//! - [`parse`] turns the XML of an assumed-valid, already-extracted content node
//!   (the body of a `<source>` / `<target>`) into an `Atom`.
//! - [`emit`] turns an `Atom` back into that XML.
//!
//! It builds no document model, validates no document, traverses no
//! `<file>` / `<trans-unit>` structure, and records no relationships — deciding
//! *which* node becomes an atom and how atoms relate is the consumer's job. The
//! one decision the parser makes per element is what the XLIFF 1.2 spec declares
//! its content to be: if it is *native code* (`<bpt>`, `<ept>`, `<ph>`, `<it>`)
//! the whole element becomes one opaque placeholder; if it is *translatable text*
//! (`<g>`, `<mrk>`) the tag is recorded as a placeholder and its inner text is
//! kept; empty inline elements (`<x/>`, `<bx/>`, `<ex/>`) are a single
//! placeholder. Placeholder markup is preserved as the raw bytes of the input and
//! is never interpreted.
//!
//! Text entities are handled per the [`Codec`] — logical (Unicode, the default)
//! or verbatim — and the same codec must be used for both halves of a round-trip.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod codec;
mod emit;
mod error;
mod parse;

pub use codec::{Codec, EntityMode};
pub use emit::emit;
pub use error::ParseError;
pub use parse::parse;
