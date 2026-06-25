//! Explicit, composable normalization primitives.
//!
//! Identity is deliberately dumb (decision D29): it hashes an
//! [`Atom`](crate::ir::Atom) exactly as recorded. Making "the same" content
//! compare equal — merging chunks, dropping empty runs, folding Unicode form,
//! trimming edges, casing a language tag — is therefore the caller's job, and
//! this module is the toolkit for it. Each function is a small, independent
//! transform with no hidden policy; the caller composes the ones it wants and
//! feeds the result to [`id_from_atom`](crate::identity::id_from_atom).
//!
//! Every content transform leaves
//! [`Placeholder`](crate::ir::ContentNode::Placeholder) nodes untouched and acts
//! only on [`Text`](crate::ir::ContentNode::Text) runs.

use unicode_normalization::UnicodeNormalization;

use crate::ir::{ContentNode, LanguageTag, LanguageTagError};

/// How [`collapse`] combines a content sequence.
pub enum CollapseMode {
    /// Merge adjacent text runs into one and drop empty runs along the way;
    /// placeholders split runs and are preserved.
    CollapseAdjacent,
    /// Drop empty text runs only, leaving run boundaries and everything else
    /// as-is.
    DropEmpty,
}

/// Which edges [`trim_nodes`] trims, and whether it trims every text run or only
/// the sequence's outer ones.
pub enum TrimMode {
    /// Trim leading characters from the first node only.
    TrimOuterLeading,
    /// Trim trailing characters from the last node only.
    TrimOuterTrailing,
    /// Trim leading from the first node and trailing from the last.
    TrimOuterBoth,
    /// Trim leading characters from every text run.
    TrimAllLeading,
    /// Trim trailing characters from every text run.
    TrimAllTrailing,
    /// Trim both ends of every text run.
    TrimAllBoth,
}

/// Collapses `content` per `mode`. Placeholders are always preserved; see
/// [`CollapseMode`] for what each mode does to text runs.
pub fn collapse(content: &[ContentNode], mode: CollapseMode) -> Vec<ContentNode> {
    match mode {
        CollapseMode::CollapseAdjacent => collapse_adjacent(content),
        CollapseMode::DropEmpty => drop_empty(content),
    }
}

/// Merges adjacent text runs into one node and drops empty ones, leaving
/// placeholders in place.
fn collapse_adjacent(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut collapsed: Vec<ContentNode> = Vec::new();
    let mut pending_text = String::new();

    for node in content {
        match node {
            ContentNode::Placeholder(_) => {
                if !pending_text.is_empty() {
                    collapsed.push(ContentNode::text(std::mem::take(&mut pending_text)));
                }
                collapsed.push(node.clone());
            }
            ContentNode::Text(data) => pending_text.push_str(data),
        }
    }
    if !pending_text.is_empty() {
        collapsed.push(ContentNode::text(pending_text));
    }

    collapsed
}

/// Removes empty text runs, leaving every other node untouched.
fn drop_empty(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut content = content.to_owned();
    content.retain(|node| match node {
        ContentNode::Placeholder(_) => true,
        ContentNode::Text(data) => !data.is_empty(),
    });
    content
}

/// Trims leading `trim` characters from a text node; placeholders pass through.
fn trim_leading(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => ContentNode::text(data.trim_start_matches(trim)),
    }
}

/// Trims trailing `trim` characters from a text node; placeholders pass through.
fn trim_trailing(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => ContentNode::text(data.trim_end_matches(trim)),
    }
}

/// Trims both ends of a text node; placeholders pass through.
fn trim_both(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => {
            let mut trimmed = data.trim_start_matches(trim);
            trimmed = trimmed.trim_end_matches(trim);
            ContentNode::text(trimmed)
        }
    }
}

/// Trims the characters in `trim` from text runs, per `mode` (see [`TrimMode`]).
///
/// `TrimOuter*` modes touch only the first and/or last node of the sequence;
/// `TrimAll*` modes touch every text run. Placeholders are never trimmed.
pub fn trim_nodes(nodes: &[ContentNode], mode: TrimMode, trim: &[char]) -> Vec<ContentNode> {
    let mut nodes = nodes.to_owned();
    match mode {
        TrimMode::TrimAllLeading => nodes.iter().map(|node| trim_leading(node, trim)).collect(),
        TrimMode::TrimAllTrailing => nodes.iter().map(|node| trim_trailing(node, trim)).collect(),
        TrimMode::TrimAllBoth => nodes.iter().map(|node| trim_both(node, trim)).collect(),
        TrimMode::TrimOuterLeading => {
            if let Some(first) = nodes.first() {
                nodes[0] = trim_leading(first, trim);
            }
            nodes
        }
        TrimMode::TrimOuterTrailing => {
            if let Some(last) = nodes.last() {
                let end = nodes.len() - 1;
                nodes[end] = trim_trailing(last, trim);
            }
            nodes
        }
        TrimMode::TrimOuterBoth => {
            if let Some(first) = nodes.first() {
                nodes[0] = trim_leading(first, trim);
            }
            if let Some(last) = nodes.last() {
                let end = nodes.len() - 1;
                nodes[end] = trim_trailing(last, trim);
            }
            nodes
        }
    }
}

/// Which Unicode normalization form [`normalize_unicode`] applies to text runs.
pub enum UnicodeNormalizationProfile {
    /// Canonical composition (NFC).
    Nfc,
    /// Compatibility composition (NFKC) — also folds compatibility characters
    /// such as ligatures and full-width forms.
    Nfkc,
    /// Canonical decomposition (NFD).
    Nfd,
    /// Compatibility decomposition (NFKD).
    Nfkd,
    /// Replaces CJK compatibility ideographs with their canonical variants.
    CjkCompatVariants,
}

/// Applies a Unicode normalization form to a single text run.
fn normalize_text(text: &str, profile: &UnicodeNormalizationProfile) -> String {
    match profile {
        UnicodeNormalizationProfile::Nfc => text.nfc().collect(),
        UnicodeNormalizationProfile::Nfkc => text.nfkc().collect(),
        UnicodeNormalizationProfile::Nfd => text.nfd().collect(),
        UnicodeNormalizationProfile::Nfkd => text.nfkd().collect(),
        UnicodeNormalizationProfile::CjkCompatVariants => text.cjk_compat_variants().collect(),
    }
}

/// Applies `profile`'s Unicode normalization form to every text run, leaving
/// placeholders untouched.
pub fn normalize_unicode(
    content: &[ContentNode],
    profile: &UnicodeNormalizationProfile,
) -> Vec<ContentNode> {
    content
        .iter()
        .map(|node| match node {
            ContentNode::Placeholder(_) => node.clone(),
            ContentNode::Text(data) => ContentNode::text(normalize_text(data, profile)),
        })
        .collect()
}

/// How [`normalize_language_tag`] cases a language tag.
pub enum LanguageNormalizationProfile {
    /// Lowercase the tag (e.g. `en-US` → `en-us`).
    Lowercase,
    /// Uppercase the tag (e.g. `en-us` → `EN-US`).
    Uppercase,
}

/// Returns a new [`LanguageTag`] with its case folded per `profile`.
///
/// The folded tag is re-parsed, so the result is validated exactly like any
/// other [`LanguageTag`].
///
/// # Errors
/// Returns [`LanguageTagError`] if the re-parse fails. In practice it does not:
/// ASCII case folding preserves both well-formedness and the region subtag, so a
/// tag built from a valid [`LanguageTag`] always re-parses.
pub fn normalize_language_tag(
    tag: &LanguageTag,
    profile: &LanguageNormalizationProfile,
) -> Result<LanguageTag, LanguageTagError> {
    match profile {
        LanguageNormalizationProfile::Lowercase => {
            LanguageTag::from_string(tag.as_str().to_ascii_lowercase())
        }
        LanguageNormalizationProfile::Uppercase => {
            LanguageTag::from_string(tag.as_str().to_ascii_uppercase())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_adjacent_merges_adjacent_text() {
        let out = collapse(
            &[ContentNode::text("Hello, "), ContentNode::text("world")],
            CollapseMode::CollapseAdjacent,
        );
        assert_eq!(out, [ContentNode::text("Hello, world")]);
    }

    #[test]
    fn collapse_adjacent_drops_empty_text() {
        let out = collapse(
            &[
                ContentNode::text(""),
                ContentNode::text("x"),
                ContentNode::text(""),
            ],
            CollapseMode::CollapseAdjacent,
        );
        assert_eq!(out, [ContentNode::text("x")]);
    }

    #[test]
    fn collapse_adjacent_keeps_placeholder_after_text() {
        // Regression: a placeholder following non-empty text must not be dropped.
        let out = collapse(
            &[
                ContentNode::text("a"),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b"),
            ],
            CollapseMode::CollapseAdjacent,
        );
        assert_eq!(
            out,
            [
                ContentNode::text("a"),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b"),
            ]
        );
    }

    #[test]
    fn collapse_adjacent_preserves_adjacent_placeholders() {
        let out = collapse(
            &[
                ContentNode::placeholder("<x id=1/>"),
                ContentNode::placeholder("<x id=2/>"),
            ],
            CollapseMode::CollapseAdjacent,
        );
        assert_eq!(
            out,
            [
                ContentNode::placeholder("<x id=1/>"),
                ContentNode::placeholder("<x id=2/>"),
            ]
        );
    }

    #[test]
    fn drop_empty_removes_empty_text_without_merging() {
        let out = collapse(
            &[
                ContentNode::text("a"),
                ContentNode::text(""),
                ContentNode::text("b"),
            ],
            CollapseMode::DropEmpty,
        );
        // Empty run gone, but the two runs are NOT merged into one.
        assert_eq!(out, [ContentNode::text("a"), ContentNode::text("b")]);
    }

    #[test]
    fn drop_empty_keeps_placeholders_including_empty_ones() {
        let out = collapse(
            &[
                ContentNode::placeholder(""),
                ContentNode::text(""),
                ContentNode::text("x"),
            ],
            CollapseMode::DropEmpty,
        );
        assert_eq!(out, [ContentNode::placeholder(""), ContentNode::text("x")]);
    }

    #[test]
    fn trim_outer_leading_trims_first_node_only() {
        let out = trim_nodes(
            &[ContentNode::text(" a"), ContentNode::text(" b")],
            TrimMode::TrimOuterLeading,
            &[' '],
        );
        assert_eq!(out, [ContentNode::text("a"), ContentNode::text(" b")]);
    }

    #[test]
    fn trim_outer_trailing_replaces_last_without_growing() {
        // Regression: must replace the last node in place, not append a copy.
        let out = trim_nodes(
            &[
                ContentNode::text("a "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b "),
            ],
            TrimMode::TrimOuterTrailing,
            &[' '],
        );
        assert_eq!(
            out,
            [
                ContentNode::text("a "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b"),
            ]
        );
    }

    #[test]
    fn trim_outer_both_trims_first_leading_and_last_trailing() {
        let out = trim_nodes(
            &[
                ContentNode::text(" a "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text(" b "),
            ],
            TrimMode::TrimOuterBoth,
            &[' '],
        );
        assert_eq!(
            out,
            [
                ContentNode::text("a "),
                ContentNode::placeholder("<x/>"),
                ContentNode::text(" b"),
            ]
        );
    }

    #[test]
    fn trim_all_both_trims_every_text_run() {
        let out = trim_nodes(
            &[ContentNode::text(" a "), ContentNode::text(" b ")],
            TrimMode::TrimAllBoth,
            &[' '],
        );
        assert_eq!(out, [ContentNode::text("a"), ContentNode::text("b")]);
    }

    #[test]
    fn trim_never_touches_placeholders() {
        let out = trim_nodes(
            &[ContentNode::placeholder(" <x/> ")],
            TrimMode::TrimOuterBoth,
            &[' '],
        );
        assert_eq!(out, [ContentNode::placeholder(" <x/> ")]);
    }

    #[test]
    fn nfc_composes_text() {
        // "e" + combining acute → "é".
        let out = normalize_unicode(
            &[ContentNode::text("e\u{0301}")],
            &UnicodeNormalizationProfile::Nfc,
        );
        assert_eq!(out, [ContentNode::text("\u{00e9}")]);
    }

    #[test]
    fn nfkc_folds_compatibility_characters() {
        // The "ﬁ" ligature folds to "fi" under NFKC.
        let out = normalize_unicode(
            &[ContentNode::text("\u{fb01}le")],
            &UnicodeNormalizationProfile::Nfkc,
        );
        assert_eq!(out, [ContentNode::text("file")]);
    }

    #[test]
    fn nfd_decomposes_text() {
        // "é" → "e" + combining acute.
        let out = normalize_unicode(
            &[ContentNode::text("\u{00e9}")],
            &UnicodeNormalizationProfile::Nfd,
        );
        assert_eq!(out, [ContentNode::text("e\u{0301}")]);
    }

    #[test]
    fn normalize_unicode_leaves_placeholders_untouched() {
        // The ligature would fold under NFKC if it were text — but it's markup.
        let out = normalize_unicode(
            &[ContentNode::placeholder("\u{fb01}")],
            &UnicodeNormalizationProfile::Nfkc,
        );
        assert_eq!(out, [ContentNode::placeholder("\u{fb01}")]);
    }

    #[test]
    fn normalize_language_tag_lowercases() {
        let tag = LanguageTag::from_string("en-US").unwrap();
        let out = normalize_language_tag(&tag, &LanguageNormalizationProfile::Lowercase).unwrap();
        assert_eq!(out.as_str(), "en-us");
    }

    #[test]
    fn normalize_language_tag_uppercases() {
        let tag = LanguageTag::from_string("en-us").unwrap();
        let out = normalize_language_tag(&tag, &LanguageNormalizationProfile::Uppercase).unwrap();
        assert_eq!(out.as_str(), "EN-US");
    }
}
