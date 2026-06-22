//! Configurable content normalization — the step between structural collapse and
//! serialization when computing an atom's identity.
//!
//! A [`NormalizationProfile`] bundles the knobs that decide exactly how a string
//! is transformed before it is hashed: which Unicode normalization form to apply
//! and how to treat edge whitespace. It is passed explicitly to the identity
//! functions, so the transformation behind an `AtomId` is always visible at the
//! call site. The profile is not part of the identity itself — its effect lives
//! entirely in the normalized bytes.

use crate::ir::{ContentNode, LanguageTag};
use unicode_normalization::UnicodeNormalization;

/// How a string is normalized before it contributes to an atom's identity.
///
/// Construct one directly, or use [`NormalizationProfile::DEFAULT`]. The knobs are
/// deliberately few and conservative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizationProfile {
    /// Which Unicode normalization form to apply to text runs.
    pub unicode: UnicodeForm,
    /// How whitespace at the outer edges of a segment is treated.
    pub whitespace: WhitespacePolicy,
}

impl NormalizationProfile {
    /// The conservative default: NFC, trimming only a segment's outer edges.
    pub const DEFAULT: NormalizationProfile = NormalizationProfile {
        unicode: UnicodeForm::Nfc,
        whitespace: WhitespacePolicy::TrimOuter,
    };
}

/// The Unicode normalization form applied to text runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeForm {
    /// Canonical composition (NFC) — lossless, the standard choice.
    Nfc,
    /// Compatibility composition (NFKC) — also folds compatibility characters
    /// such as ligatures and full-width forms. More aggressive.
    Nfkc,
}

/// How whitespace at the outer edges of a segment is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhitespacePolicy {
    /// Leave all whitespace exactly as recorded.
    Preserve,
    /// Trim leading whitespace from the segment's first text run and trailing
    /// whitespace from its last. Whitespace inside the segment — including next to
    /// placeholders — is preserved.
    TrimOuter,
}

/// Normalizes the text content of a collapsed node sequence.
///
/// Applies the profile's Unicode form to each text run, then — under
/// [`WhitespacePolicy::TrimOuter`] — trims the segment's outer edges, dropping any
/// run left empty. Placeholders pass through untouched. Expects already-collapsed
/// input (see [`crate::identity::collapse`]).
pub fn normalize_content(
    content: &[ContentNode],
    profile: &NormalizationProfile,
) -> Vec<ContentNode> {
    let mut nodes: Vec<ContentNode> = content
        .iter()
        .map(|node| {
            if node.is_placeholder() {
                node.clone()
            } else {
                ContentNode::text(normalize_text(node.data(), profile.unicode))
            }
        })
        .collect();

    if profile.whitespace == WhitespacePolicy::TrimOuter {
        trim_outer(&mut nodes);
    }

    nodes
}

/// Normalizes a language tag for hashing by lowercasing it. BCP-47 tags are
/// ASCII, so this is deterministic.
pub fn normalize_language(tag: &LanguageTag) -> String {
    tag.as_str().to_ascii_lowercase()
}

/// Applies a Unicode normalization form to a text run.
fn normalize_text(text: &str, form: UnicodeForm) -> String {
    match form {
        UnicodeForm::Nfc => text.nfc().collect(),
        UnicodeForm::Nfkc => text.nfkc().collect(),
    }
}

/// Trims leading whitespace from the first text run and trailing whitespace from
/// the last, dropping a run left empty. Internal whitespace is untouched.
fn trim_outer(nodes: &mut Vec<ContentNode>) {
    let leading = match nodes.first() {
        Some(node) if !node.is_placeholder() => {
            let trimmed = node.data().trim_start();
            (trimmed.len() != node.data().len()).then(|| trimmed.to_owned())
        }
        _ => None,
    };
    if let Some(trimmed) = leading {
        if trimmed.is_empty() {
            nodes.remove(0);
        } else {
            nodes[0] = ContentNode::text(trimmed);
        }
    }

    let trailing = match nodes.last() {
        Some(node) if !node.is_placeholder() => {
            let trimmed = node.data().trim_end();
            (trimmed.len() != node.data().len()).then(|| trimmed.to_owned())
        }
        _ => None,
    };
    if let Some(trimmed) = trailing {
        if trimmed.is_empty() {
            nodes.pop();
        } else {
            let last = nodes.len() - 1;
            nodes[last] = ContentNode::text(trimmed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NFC_PRESERVE: NormalizationProfile = NormalizationProfile {
        unicode: UnicodeForm::Nfc,
        whitespace: WhitespacePolicy::Preserve,
    };
    const NFKC_TRIM: NormalizationProfile = NormalizationProfile {
        unicode: UnicodeForm::Nfkc,
        whitespace: WhitespacePolicy::TrimOuter,
    };

    #[test]
    fn nfc_composes_text() {
        // "e" + combining acute → "é".
        let out = normalize_content(&[ContentNode::text("e\u{0301}")], &NFC_PRESERVE);
        assert_eq!(out, [ContentNode::text("\u{00e9}")]);
    }

    #[test]
    fn nfkc_folds_compatibility_characters() {
        // The "ﬁ" ligature folds to "fi" under NFKC, not under NFC.
        let nfkc = normalize_content(&[ContentNode::text("\u{fb01}le")], &NFKC_TRIM);
        assert_eq!(nfkc, [ContentNode::text("file")]);
        let nfc = normalize_content(&[ContentNode::text("\u{fb01}le")], &NFC_PRESERVE);
        assert_eq!(nfc, [ContentNode::text("\u{fb01}le")]);
    }

    #[test]
    fn trim_outer_trims_segment_edges_only() {
        let out = normalize_content(
            &[
                ContentNode::text(" Hello "),
                ContentNode::placeholder("<b>"),
                ContentNode::text(" world "),
            ],
            &NormalizationProfile::DEFAULT,
        );
        // Leading of the first run and trailing of the last gone; internal kept.
        assert_eq!(
            out,
            [
                ContentNode::text("Hello "),
                ContentNode::placeholder("<b>"),
                ContentNode::text(" world"),
            ]
        );
    }

    #[test]
    fn trim_outer_drops_whitespace_only_edges() {
        let out = normalize_content(
            &[
                ContentNode::text("   "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("  "),
            ],
            &NormalizationProfile::DEFAULT,
        );
        assert_eq!(out, [ContentNode::placeholder("<x/>")]);
    }

    #[test]
    fn trim_outer_whitespace_only_single_run_becomes_empty() {
        let out = normalize_content(&[ContentNode::text("   ")], &NormalizationProfile::DEFAULT);
        assert!(out.is_empty());
    }

    #[test]
    fn preserve_keeps_outer_whitespace() {
        let out = normalize_content(&[ContentNode::text(" Hello ")], &NFC_PRESERVE);
        assert_eq!(out, [ContentNode::text(" Hello ")]);
    }

    #[test]
    fn internal_whitespace_is_never_collapsed() {
        let out = normalize_content(
            &[ContentNode::text("Hello   world")],
            &NormalizationProfile::DEFAULT,
        );
        assert_eq!(out, [ContentNode::text("Hello   world")]);
    }

    #[test]
    fn normalize_language_lowercases() {
        let tag = LanguageTag::parse("en-US").unwrap();
        assert_eq!(normalize_language(&tag), "en-us");
    }
}
