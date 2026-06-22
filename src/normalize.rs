//! Configurable content normalization — the step between structural collapse and
//! serialization when computing an atom's identity.
//!
//! A [`NormalizationProfile`] bundles the knobs that decide exactly how a string
//! is transformed before it is hashed: which Unicode normalization form to apply
//! and which code points to trim from a segment's outer edges. It is passed
//! explicitly to the identity functions, so the transformation behind an
//! `AtomId` is always visible at the call site. The profile is not part of the
//! identity itself — its effect lives entirely in the normalized bytes.

use crate::ir::{ContentNode, LanguageTag};
use unicode_normalization::UnicodeNormalization;

/// How a string is normalized before it contributes to an atom's identity.
///
/// Construct one directly, or use [`NormalizationProfile::default`]. Every input
/// to the identity hash is captured here explicitly — there is no reliance on a
/// library's notion of "whitespace" or similar ambient behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationProfile {
    /// Which Unicode normalization form to apply to text runs.
    pub unicode: UnicodeForm,
    /// Code points trimmed from a segment's outer edges. Leading and trailing
    /// runs of these characters are removed, stopping at the first character not
    /// in the set. An empty set trims nothing. Content internal to the segment —
    /// including next to placeholders — is always preserved.
    pub edge_trim: Vec<char>,
}

impl Default for NormalizationProfile {
    /// The conservative default: NFC, trimming ASCII tab, newline, carriage
    /// return, and space from a segment's outer edges.
    fn default() -> Self {
        Self {
            unicode: UnicodeForm::Nfc,
            edge_trim: vec!['\t', '\n', '\r', ' '],
        }
    }
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

/// Normalizes the text content of a collapsed node sequence.
///
/// Applies the profile's Unicode form to each text run, then trims the segment's
/// outer edges of any characters in `edge_trim`, dropping a run left empty.
/// Placeholders pass through untouched. Expects already-collapsed input (see
/// [`crate::identity::collapse`]).
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

    if !profile.edge_trim.is_empty() {
        trim_outer(&mut nodes, &profile.edge_trim);
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

/// Trims leading characters in `trim` from the first text run and trailing ones
/// from the last, dropping a run left empty. Internal content is untouched.
fn trim_outer(nodes: &mut Vec<ContentNode>, trim: &[char]) {
    let leading = match nodes.first() {
        Some(node) if !node.is_placeholder() => {
            let trimmed = node.data().trim_start_matches(|c| trim.contains(&c));
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
            let trimmed = node.data().trim_end_matches(|c| trim.contains(&c));
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

    fn nfc_no_trim() -> NormalizationProfile {
        NormalizationProfile {
            unicode: UnicodeForm::Nfc,
            edge_trim: vec![],
        }
    }

    fn nfkc_default_trim() -> NormalizationProfile {
        NormalizationProfile {
            unicode: UnicodeForm::Nfkc,
            edge_trim: vec!['\t', '\n', '\r', ' '],
        }
    }

    #[test]
    fn nfc_composes_text() {
        // "e" + combining acute → "é".
        let out = normalize_content(&[ContentNode::text("e\u{0301}")], &nfc_no_trim());
        assert_eq!(out, [ContentNode::text("\u{00e9}")]);
    }

    #[test]
    fn nfkc_folds_compatibility_characters() {
        // The "ﬁ" ligature folds to "fi" under NFKC, not under NFC.
        let nfkc = normalize_content(&[ContentNode::text("\u{fb01}le")], &nfkc_default_trim());
        assert_eq!(nfkc, [ContentNode::text("file")]);
        let nfc = normalize_content(&[ContentNode::text("\u{fb01}le")], &nfc_no_trim());
        assert_eq!(nfc, [ContentNode::text("\u{fb01}le")]);
    }

    #[test]
    fn default_trims_segment_edges_only() {
        let out = normalize_content(
            &[
                ContentNode::text(" Hello "),
                ContentNode::placeholder("<b>"),
                ContentNode::text(" world "),
            ],
            &NormalizationProfile::default(),
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
    fn default_drops_whitespace_only_edges() {
        let out = normalize_content(
            &[
                ContentNode::text("   "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("  "),
            ],
            &NormalizationProfile::default(),
        );
        assert_eq!(out, [ContentNode::placeholder("<x/>")]);
    }

    #[test]
    fn whitespace_only_single_run_becomes_empty() {
        let out = normalize_content(
            &[ContentNode::text("   ")],
            &NormalizationProfile::default(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn empty_trim_set_preserves_edges() {
        let out = normalize_content(&[ContentNode::text(" Hello ")], &nfc_no_trim());
        assert_eq!(out, [ContentNode::text(" Hello ")]);
    }

    #[test]
    fn internal_content_is_never_trimmed() {
        let out = normalize_content(
            &[ContentNode::text("Hello   world")],
            &NormalizationProfile::default(),
        );
        assert_eq!(out, [ContentNode::text("Hello   world")]);
    }

    #[test]
    fn default_set_does_not_trim_nbsp() {
        // The default set is exactly {tab, LF, CR, space}: NBSP (U+00A0) is left
        // alone, so meaningful non-breaking spacing survives at the edges.
        let out = normalize_content(
            &[ContentNode::text(" \u{00a0}hi\u{00a0} ")],
            &NormalizationProfile::default(),
        );
        assert_eq!(out, [ContentNode::text("\u{00a0}hi\u{00a0}")]);
    }

    #[test]
    fn custom_trim_set_trims_arbitrary_characters() {
        let profile = NormalizationProfile {
            unicode: UnicodeForm::Nfc,
            edge_trim: vec!['!'],
        };
        let out = normalize_content(&[ContentNode::text("!!hi!!")], &profile);
        assert_eq!(out, [ContentNode::text("hi")]);
    }

    #[test]
    fn normalize_language_lowercases() {
        let tag = LanguageTag::parse("en-US").unwrap();
        assert_eq!(normalize_language(&tag), "en-us");
    }
}
