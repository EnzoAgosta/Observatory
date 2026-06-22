//! Configurable content normalization (D6) — the layer between structural
//! collapse and serialization in the `AtomId` pipeline.
//!
//! A [`NormalizationProfile`] is a small, transparent bundle of knobs deciding
//! exactly how a string is transformed before hashing. It is always passed
//! *explicitly* to the identity functions, never implicitly defaulted (D20): the
//! transformation is integral to how an `AtomId` is derived, so it must be
//! visible at the call site. The profile is **not** part of the hash (D20) — its
//! effect is already in the normalized bytes; provenance is storage-layer
//! metadata.

use crate::ir::{ContentNode, LanguageTag};
use unicode_normalization::UnicodeNormalization;

/// How a string is normalized before it contributes to an `AtomId` (D6, D20).
///
/// Construct one explicitly, or pass [`NormalizationProfile::DEFAULT`]. The knobs
/// are deliberately few and conservative; more are added only as need is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizationProfile {
    /// Which Unicode normalization form to apply to text runs.
    pub unicode: UnicodeForm,
    /// How whitespace at the outer edges of a segment is treated.
    pub whitespace: WhitespacePolicy,
}

impl NormalizationProfile {
    /// The conservative default: NFC, trimming only the segment's outer edges.
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
    /// (ligatures, full-width forms, …). More aggressive; opt-in.
    Nfkc,
}

/// How whitespace at the outer edges of a segment is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhitespacePolicy {
    /// Leave all whitespace exactly as recorded.
    Preserve,
    /// Trim leading whitespace from the segment's first text run and trailing
    /// whitespace from its last. Whitespace internal to the segment — including
    /// next to placeholders — is preserved (D20).
    TrimOuter,
}

/// Normalizes the text content of a (collapsed) node sequence for hashing (D6).
///
/// Applies the profile's Unicode form to each text run, then — for
/// [`WhitespacePolicy::TrimOuter`] — trims the segment's outer edges, dropping
/// any run left empty. Placeholders pass through untouched (D16).
///
/// Expects already-[collapsed](crate::identity::collapse) input.
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

/// Normalizes a language tag for hashing: lowercased (D7). The tag is BCP-47, so
/// ASCII-lowercasing is sufficient and deterministic.
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
        // Leading of first and trailing of last gone; internal spaces kept.
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
