//! Content addressing: turning an [`Atom`] into a stable [`AtomId`].
//!
//! An atom's identity is a SHA-256 digest over a canonical serialization of its
//! normalized content and language. The pipeline is:
//!
//! 1. **collapse** ([`collapse`]) — the fixed structural step: merge adjacent
//!    text runs and drop empty ones, leaving placeholders untouched.
//! 2. **normalize** ([`crate::normalize`]) — apply the caller's
//!    [`NormalizationProfile`] (Unicode form, whitespace) and lowercase the
//!    language tag.
//! 3. **serialize** ([`canonical_bytes`]) — lay the result out as unambiguous,
//!    length-prefixed bytes.
//! 4. **hash** ([`atom_id`]) — SHA-256 of those bytes.
//!
//! Only normalized content and the language tag feed the hash. The normalization
//! profile is passed explicitly but is not itself part of the identity — its
//! effect is already captured in the normalized bytes.

use crate::ir::{Atom, ContentNode};
use crate::normalize::{NormalizationProfile, normalize_content, normalize_language};
use sha2::{Digest, Sha256};
use std::fmt;

/// Version of the canonical serialization scheme, hashed in-band so that a future
/// scheme can never collide with this one in `AtomId` space.
const SERIALIZATION_VERSION: u8 = 1;

/// Serialization tag byte marking a text node.
const TAG_TEXT: u8 = 0x00;
/// Serialization tag byte marking a placeholder node.
const TAG_PLACEHOLDER: u8 = 0x01;

/// The content-addressed identity of an [`Atom`]: the 32-byte SHA-256 digest of
/// its [`canonical_bytes`].
///
/// Two atoms have the same `AtomId` exactly when their normalized content and
/// language match — independent of inline-tag markup, how the text was split into
/// runs, or the case of the language tag. This is the value to compare and key
/// by, not structural atom equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomId([u8; 32]);

impl AtomId {
    /// The raw 32-byte digest.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for AtomId {
    /// Renders the digest as 64 lowercase hex characters.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Collapses a content sequence into its canonical structural form: adjacent text
/// runs are merged and empty text runs dropped. Placeholders are left exactly as
/// they are — their count and position carry meaning.
///
/// This is the structural step of computing an identity; it does not normalize
/// text *content* (Unicode form, whitespace, case), which is the separate,
/// configurable job of [`crate::normalize`]. Collapsing is idempotent.
pub fn collapse(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut collapsed: Vec<ContentNode> = Vec::new();
    let mut pending_text = String::new();

    for node in content {
        if node.is_placeholder() {
            flush_text(&mut pending_text, &mut collapsed);
            collapsed.push(node.clone());
        } else {
            pending_text.push_str(node.data());
        }
    }
    flush_text(&mut pending_text, &mut collapsed);

    collapsed
}

/// Pushes the accumulated text as a single node unless it is empty, and clears
/// the accumulator.
fn flush_text(pending: &mut String, out: &mut Vec<ContentNode>) {
    if !pending.is_empty() {
        out.push(ContentNode::text(std::mem::take(pending)));
    }
}

/// Serializes `atom` into its canonical byte form — the exact input to the
/// identity hash. The content is collapsed and then normalized with `profile`.
///
/// The layout is unambiguous and length-prefixed: a scheme-version byte; then the
/// lowercased language tag as a 32-bit big-endian length followed by its UTF-8
/// bytes; then each content node — a text run as a tag byte, a 32-bit big-endian
/// length, and its UTF-8 bytes, and a placeholder as a single tag byte (its
/// markup is deliberately excluded, so only its presence and position count).
///
/// # Panics
/// Panics if the language tag or a single text run exceeds `u32::MAX` bytes
/// (~4 GiB) — far beyond any real segment.
pub fn canonical_bytes(atom: &Atom, profile: &NormalizationProfile) -> Vec<u8> {
    let mut buf = vec![SERIALIZATION_VERSION];
    write_bytes_field(&mut buf, normalize_language(atom.language()).as_bytes());

    for node in normalize_content(&collapse(atom.content()), profile) {
        if node.is_placeholder() {
            buf.push(TAG_PLACEHOLDER);
        } else {
            buf.push(TAG_TEXT);
            write_bytes_field(&mut buf, node.data().as_bytes());
        }
    }

    buf
}

/// Appends a `u32` big-endian length prefix followed by `bytes`.
fn write_bytes_field(buf: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("field exceeds u32::MAX bytes");
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(bytes);
}

/// Computes the [`AtomId`] of `atom` under `profile`: the SHA-256 of its
/// [`canonical_bytes`].
pub fn atom_id(atom: &Atom, profile: &NormalizationProfile) -> AtomId {
    let digest = Sha256::digest(canonical_bytes(atom, profile));
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest[..]);
    AtomId(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::LanguageTag;
    use crate::normalize::UnicodeForm;

    /// Builds an `en-us` atom from the given nodes — keeps the tests terse.
    fn en(nodes: impl IntoIterator<Item = ContentNode>) -> Atom {
        Atom::new(LanguageTag::parse("en-us").unwrap(), nodes)
    }

    /// The default-profile id, for tests that don't probe the profile.
    fn id(atom: &Atom) -> AtomId {
        atom_id(atom, &NormalizationProfile::default())
    }

    /// NFC, no edge trimming.
    fn no_trim() -> NormalizationProfile {
        NormalizationProfile {
            unicode: UnicodeForm::Nfc,
            edge_trim: vec![],
        }
    }

    /// NFKC, default edge trimming.
    fn nfkc() -> NormalizationProfile {
        NormalizationProfile {
            unicode: UnicodeForm::Nfkc,
            edge_trim: vec!['\t', '\n', '\r', ' '],
        }
    }

    #[test]
    fn merges_adjacent_text_into_one_node() {
        let collapsed = collapse(&[ContentNode::text("Hello, "), ContentNode::text("world")]);
        assert_eq!(collapsed, [ContentNode::text("Hello, world")]);
    }

    #[test]
    fn drops_empty_text_runs() {
        let collapsed = collapse(&[
            ContentNode::text(""),
            ContentNode::text("x"),
            ContentNode::text(""),
        ]);
        assert_eq!(collapsed, [ContentNode::text("x")]);
    }

    #[test]
    fn placeholders_split_text_runs() {
        let collapsed = collapse(&[
            ContentNode::text("a"),
            ContentNode::placeholder("<x/>"),
            ContentNode::text("b"),
        ]);
        assert_eq!(
            collapsed,
            [
                ContentNode::text("a"),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b"),
            ]
        );
    }

    #[test]
    fn adjacent_placeholders_are_preserved() {
        let collapsed = collapse(&[
            ContentNode::placeholder("<x id=1/>"),
            ContentNode::placeholder("<x id=2/>"),
        ]);
        assert_eq!(collapsed.len(), 2);
        assert!(collapsed.iter().all(ContentNode::is_placeholder));
    }

    #[test]
    fn empty_placeholder_is_preserved() {
        let collapsed = collapse(&[ContentNode::placeholder("")]);
        assert_eq!(collapsed, [ContentNode::placeholder("")]);
    }

    #[test]
    fn distinct_chunkings_collapse_identically() {
        let a = collapse(&[ContentNode::text("a"), ContentNode::text("b")]);
        let b = collapse(&[ContentNode::text("ab")]);
        assert_eq!(a, b);
    }

    #[test]
    fn collapse_is_idempotent() {
        let once = collapse(&[
            ContentNode::text("a"),
            ContentNode::text(""),
            ContentNode::text("b"),
            ContentNode::placeholder("<x/>"),
            ContentNode::placeholder(""),
            ContentNode::text("c"),
        ]);
        let twice = collapse(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn collapse_preserves_the_joined_text() {
        let raw = [
            ContentNode::text("Click "),
            ContentNode::text(""),
            ContentNode::placeholder("<g id=1>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ];
        let joined: String = raw.iter().map(ContentNode::data).collect();
        let collapsed_joined: String = collapse(&raw).iter().map(ContentNode::data).collect();
        assert_eq!(joined, collapsed_joined);
    }

    #[test]
    fn canonical_bytes_matches_the_spec_layout() {
        let atom = en([
            ContentNode::text("Click "),
            ContentNode::placeholder("<g id=1>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ]);

        let mut expected = vec![1u8]; // version
        expected.extend_from_slice(&5u32.to_be_bytes()); // "en-us"
        expected.extend_from_slice(b"en-us");
        expected.push(0x00); // text
        expected.extend_from_slice(&6u32.to_be_bytes());
        expected.extend_from_slice(b"Click ");
        expected.push(0x01); // placeholder, no data
        expected.push(0x00); // text
        expected.extend_from_slice(&4u32.to_be_bytes());
        expected.extend_from_slice(b"here");
        expected.push(0x01); // placeholder, no data

        assert_eq!(
            canonical_bytes(&atom, &NormalizationProfile::default()),
            expected
        );
    }

    #[test]
    fn distinct_chunkings_have_the_same_id() {
        // Structurally different yet identical identity.
        let chunked = en([ContentNode::text("a"), ContentNode::text("b")]);
        let merged = en([ContentNode::text("ab")]);
        assert_ne!(chunked, merged);
        assert_eq!(id(&chunked), id(&merged));
    }

    #[test]
    fn placeholder_data_does_not_affect_id() {
        // A placeholder contributes only its presence, not its markup.
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
        assert_eq!(id(&bold), id(&vars));
    }

    #[test]
    fn structure_changes_the_id() {
        // Same text, different placeholder count/position → different identity.
        let two_placeholders = en([
            ContentNode::placeholder("<b>"),
            ContentNode::text("x"),
            ContentNode::placeholder("</b>"),
        ]);
        let one_placeholder = en([ContentNode::text("x"), ContentNode::placeholder("<br/>")]);
        assert_ne!(id(&two_placeholders), id(&one_placeholder));
    }

    #[test]
    fn language_changes_the_id() {
        // Same bytes, different language → different id (e.g. the en/de "Gift").
        let english = en([ContentNode::text("Gift")]);
        let german = Atom::new(
            LanguageTag::parse("de-de").unwrap(),
            [ContentNode::text("Gift")],
        );
        assert_ne!(id(&english), id(&german));
    }

    #[test]
    fn language_case_does_not_affect_id() {
        // The tag is lowercased when computing identity, so case can't fragment.
        let upper = Atom::new(
            LanguageTag::parse("en-US").unwrap(),
            [ContentNode::text("hi")],
        );
        let lower = Atom::new(
            LanguageTag::parse("en-us").unwrap(),
            [ContentNode::text("hi")],
        );
        assert_ne!(upper, lower); // structurally distinct (faithful tags)
        assert_eq!(id(&upper), id(&lower)); // ... same identity
    }

    #[test]
    fn outer_whitespace_does_not_affect_id_under_default() {
        assert_eq!(
            id(&en([ContentNode::text(" hi ")])),
            id(&en([ContentNode::text("hi")]))
        );
    }

    #[test]
    fn internal_whitespace_changes_the_id() {
        assert_ne!(
            id(&en([ContentNode::text("Hello  world")])),
            id(&en([ContentNode::text("Hello world")]))
        );
    }

    #[test]
    fn empty_trim_set_changes_the_id() {
        // With no edge trimming, outer whitespace is kept and so distinguishes.
        let spaced = en([ContentNode::text(" hi ")]);
        let tight = en([ContentNode::text("hi")]);
        assert_ne!(atom_id(&spaced, &no_trim()), atom_id(&tight, &no_trim()));
    }

    #[test]
    fn unicode_form_changes_the_id() {
        // NFKC folds the "ﬁ" ligature into "fi"; NFC does not.
        let ligature = en([ContentNode::text("\u{fb01}le")]);
        let plain = en([ContentNode::text("file")]);
        assert_eq!(atom_id(&ligature, &nfkc()), atom_id(&plain, &nfkc()));
        assert_ne!(id(&ligature), id(&plain)); // default is NFC
    }

    #[test]
    fn atom_id_is_deterministic() {
        let atom = en([ContentNode::text("hello"), ContentNode::placeholder("<x/>")]);
        assert_eq!(id(&atom), id(&atom));
    }

    #[test]
    fn atom_id_displays_as_64_lowercase_hex() {
        let hex = id(&en([ContentNode::text("hello")])).to_string();
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn atom_id_exposes_32_bytes() {
        assert_eq!(id(&en([ContentNode::text("x")])).as_bytes().len(), 32);
    }
}
