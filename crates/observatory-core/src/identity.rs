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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::LanguageTag;

    /// Builds an `en-US` atom from the given nodes — keeps the tests terse.
    fn en(nodes: impl IntoIterator<Item = ContentNode>) -> Atom {
        Atom::new(LanguageTag::from_string("en-US").unwrap(), nodes)
    }

    #[test]
    fn id_is_deterministic() {
        let atom = en([ContentNode::text("hello"), ContentNode::placeholder("<x/>")]);
        assert_eq!(id_from_atom(&atom), id_from_atom(&atom));
    }

    #[test]
    fn different_text_changes_the_id() {
        assert_ne!(
            id_from_atom(&en([ContentNode::text("a")])),
            id_from_atom(&en([ContentNode::text("b")])),
        );
    }

    #[test]
    fn placeholder_markup_does_not_affect_the_id() {
        // Identity excludes placeholder markup — only its presence and position
        // count, so two taggings of the same text agree.
        let bold = en([
            ContentNode::text("Click "),
            ContentNode::placeholder("<b>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</b>"),
        ]);
        let vars = en([
            ContentNode::text("Click "),
            ContentNode::placeholder("{0}"),
            ContentNode::text("here"),
            ContentNode::placeholder("{1}"),
        ]);
        assert_eq!(id_from_atom(&bold), id_from_atom(&vars));
    }

    #[test]
    fn placeholder_count_and_position_change_the_id() {
        let two = en([
            ContentNode::placeholder("<b>"),
            ContentNode::text("x"),
            ContentNode::placeholder("</b>"),
        ]);
        let one = en([ContentNode::text("x"), ContentNode::placeholder("<br/>")]);
        assert_ne!(id_from_atom(&two), id_from_atom(&one));
    }

    #[test]
    fn text_chunking_changes_the_id() {
        // D29: identity does not collapse, so how text is split is significant.
        let merged = en([ContentNode::text("ab")]);
        let split = en([ContentNode::text("a"), ContentNode::text("b")]);
        assert_ne!(id_from_atom(&merged), id_from_atom(&split));
    }

    #[test]
    fn empty_text_run_changes_the_id() {
        let with_empty = en([ContentNode::text(""), ContentNode::text("x")]);
        let without = en([ContentNode::text("x")]);
        assert_ne!(id_from_atom(&with_empty), id_from_atom(&without));
    }

    #[test]
    fn surrounding_whitespace_changes_the_id() {
        // D29: identity does not trim or fold whitespace.
        assert_ne!(
            id_from_atom(&en([ContentNode::text(" hi ")])),
            id_from_atom(&en([ContentNode::text("hi")])),
        );
    }

    #[test]
    fn language_case_changes_the_id() {
        // D29: the tag is serialized verbatim, so case is significant.
        let upper = en([ContentNode::text("hi")]);
        let lower = Atom::new(
            LanguageTag::from_string("en-us").unwrap(),
            [ContentNode::text("hi")],
        );
        assert_ne!(id_from_atom(&upper), id_from_atom(&lower));
    }

    #[test]
    fn different_language_changes_the_id() {
        // Same bytes, different language — e.g. the en/de "Gift".
        let english = en([ContentNode::text("Gift")]);
        let german = Atom::new(
            LanguageTag::from_string("de-DE").unwrap(),
            [ContentNode::text("Gift")],
        );
        assert_ne!(id_from_atom(&english), id_from_atom(&german));
    }

    #[test]
    fn canonical_bytes_matches_the_layout() {
        let atom = en([
            ContentNode::text("Click "),
            ContentNode::placeholder("<g id=1>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ]);

        let mut expected = vec![SERIALIZATION_VERSION];
        expected.extend_from_slice(&5u32.to_be_bytes()); // "en-US"
        expected.extend_from_slice(b"en-US");
        expected.push(TAG_TEXT);
        expected.extend_from_slice(&6u32.to_be_bytes());
        expected.extend_from_slice(b"Click ");
        expected.push(TAG_PLACEHOLDER); // markup excluded
        expected.push(TAG_TEXT);
        expected.extend_from_slice(&4u32.to_be_bytes());
        expected.extend_from_slice(b"here");
        expected.push(TAG_PLACEHOLDER);

        assert_eq!(
            canonical_bytes(atom.language().as_str(), atom.content()),
            expected
        );
    }

    #[test]
    fn canonical_bytes_of_empty_atom_is_version_plus_language() {
        let atom = en([]);
        let mut expected = vec![SERIALIZATION_VERSION];
        expected.extend_from_slice(&5u32.to_be_bytes());
        expected.extend_from_slice(b"en-US");
        assert_eq!(
            canonical_bytes(atom.language().as_str(), atom.content()),
            expected
        );
    }

    #[test]
    fn atom_id_round_trips_through_its_digest() {
        let id = id_from_atom(&en([ContentNode::text("hello")]));
        assert_eq!(AtomId::from_digest(id.digest()), id);
    }

    #[test]
    fn digest_is_32_bytes() {
        assert_eq!(
            id_from_atom(&en([ContentNode::text("x")])).digest().len(),
            32
        );
    }
}
