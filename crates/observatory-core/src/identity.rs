//! Content addressing: turning an [`Atom`] into a stable [`AtomId`].
//!
//! The id is a SHA-256 digest over a canonical, length-prefixed serialization of
//! the atom *exactly as recorded* — its language tag (verbatim) and its content
//! nodes. Identity is deliberately dumb (decision D29): it performs no
//! normalization and no structural collapse, so text, chunking, and language case
//! are all significant. The one thing it excludes is placeholder *markup* — a
//! placeholder contributes only its presence and position, never its bytes — so
//! the same text tagged two different ways still shares an id.
//!
//! Canonicalizing "the same" content across taggings (merging chunks, folding
//! whitespace or case, …) is the caller's job, applied with [`crate::normalize`]
//! before calling [`id_from_atom`].
//!
//! ## Serialization layout
//!
//! ```text
//! [version: u8]
//! [lang-len: u32 BE][lang-bytes: UTF-8]
//! ( node )*
//!   text node:        [0x00][len: u32 BE][UTF-8 bytes]
//!   placeholder node: [0x01]                          (no data)
//! ```
//!
//! Lengths are fixed-width big-endian, so ids are identical across platforms, and
//! the framing is self-delimiting (no escaping needed). The leading version byte
//! is hashed in-band, so a future scheme can never collide with this one.

use sha2::{Digest, Sha256};

use crate::ir::{Atom, ContentNode};

/// Version of the canonical serialization scheme, hashed in-band so a future
/// scheme can never collide with this one in `AtomId` space.
const SERIALIZATION_VERSION: u8 = 0;

/// Tag byte marking a text node; followed by a length-prefixed UTF-8 run.
const TAG_TEXT: u8 = 0x00;

/// Tag byte marking a placeholder node; carries no data, so only the
/// placeholder's presence and position enter the id.
const TAG_PLACEHOLDER: u8 = 0x01;

/// The content-addressed identity of an [`Atom`]: the 32-byte SHA-256 digest of
/// its canonical serialization.
///
/// Two atoms share an `AtomId` exactly when their language tags and content match
/// as recorded, except that placeholder *markup* is ignored (only placeholder
/// position and count matter). There is no normalization — fold content with
/// [`crate::normalize`] first if you want case, whitespace, or chunking
/// differences to collapse. This is the value to compare and key by, not
/// structural `Atom` equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomId([u8; 32]);

impl AtomId {
    /// Wraps a raw 32-byte digest as an `AtomId` — e.g. one read back from
    /// storage.
    pub fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw 32-byte digest.
    pub fn digest(&self) -> [u8; 32] {
        self.0
    }
}

/// Serializes the content nodes: each text run as a tag byte plus a
/// length-prefixed UTF-8 field, each placeholder as a lone tag byte (its markup
/// is deliberately excluded from identity).
fn content_as_bytes(content: &[ContentNode]) -> Vec<u8> {
    let mut buf = Vec::new();
    for node in content {
        match node {
            ContentNode::Placeholder(_) => {
                buf.push(TAG_PLACEHOLDER);
            }
            ContentNode::Text(data) => {
                buf.push(TAG_TEXT);
                add_bytes_with_be_length(&mut buf, data.as_bytes());
            }
        }
    }
    buf
}

/// Appends a `u32` big-endian length prefix followed by `bytes`.
///
/// # Panics
/// Panics if `bytes` is longer than `u32::MAX` (~4 GiB) — far beyond any real
/// segment.
fn add_bytes_with_be_length(buf: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("field exceeds u32::MAX bytes");
    buf.extend(len.to_be_bytes());
    buf.extend(bytes);
}

/// Serializes the language tag as a single length-prefixed field, verbatim.
fn language_as_bytes(language: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    add_bytes_with_be_length(&mut buf, language.as_bytes());
    buf
}

/// Lays out the full canonical byte string — version, then language, then
/// content — per the layout in the module docs.
fn canonical_bytes(language: &str, content: &[ContentNode]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(SERIALIZATION_VERSION);
    buf.extend(language_as_bytes(language));
    buf.extend(content_as_bytes(content));
    buf
}

/// Computes the [`AtomId`] of `atom`: the SHA-256 of its canonical serialization.
///
/// Pure in `atom` — the same atom always yields the same id. Applies no
/// normalization; canonicalize with [`crate::normalize`] first if you need it.
pub fn id_from_atom(atom: &Atom) -> AtomId {
    let bytes = canonical_bytes(atom.language().as_str(), atom.content());
    let digest = Sha256::digest(bytes);
    AtomId::from_digest(digest.into())
}
