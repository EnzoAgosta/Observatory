//! Property-based invariants for the normalize primitives and their effect on
//! identity.
//!
//! These hold over randomly generated content: collapsing is idempotent, lossless
//! over the joined text, and yields a canonical structure; normalization leaves
//! placeholders alone; and — the headline — once a caller collapses, an atom's id
//! no longer depends on how its text was split into runs.

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_core::normalize::{
    CollapseMode, TrimMode, UnicodeNormalizationProfile, collapse, normalize_unicode, trim_nodes,
};
use proptest::prelude::*;

fn arb_node() -> impl Strategy<Value = ContentNode> {
    prop_oneof![
        any::<String>().prop_map(ContentNode::text),
        any::<String>().prop_map(ContentNode::placeholder),
    ]
}

fn arb_nodes() -> impl Strategy<Value = Vec<ContentNode>> {
    prop::collection::vec(arb_node(), 0..12)
}

fn en(nodes: Vec<ContentNode>) -> Atom {
    Atom::new(LanguageTag::from_string("en-US").unwrap(), nodes)
}

proptest! {
    /// Collapsing twice is the same as collapsing once.
    #[test]
    fn collapse_adjacent_is_idempotent(nodes in arb_nodes()) {
        let once = collapse(&nodes, CollapseMode::CollapseAdjacent);
        let twice = collapse(&once, CollapseMode::CollapseAdjacent);
        prop_assert_eq!(once, twice);
    }

    /// Collapsing adjacent text never changes the string an atom reconstructs to.
    #[test]
    fn collapse_adjacent_preserves_joined_text(nodes in arb_nodes()) {
        let before: String = nodes.iter().map(ContentNode::as_str).collect();
        let after: String = collapse(&nodes, CollapseMode::CollapseAdjacent)
            .iter()
            .map(ContentNode::as_str)
            .collect();
        prop_assert_eq!(before, after);
    }

    /// `CollapseAdjacent` output is canonical: no empty text run, and never two
    /// text runs in a row.
    #[test]
    fn collapse_adjacent_yields_canonical_structure(nodes in arb_nodes()) {
        let out = collapse(&nodes, CollapseMode::CollapseAdjacent);
        for node in &out {
            if let ContentNode::Text(data) = node {
                prop_assert!(!data.is_empty());
            }
        }
        for pair in out.windows(2) {
            prop_assert!(!matches!(pair, [ContentNode::Text(_), ContentNode::Text(_)]));
        }
    }

    /// `DropEmpty` removes every empty text run and changes nothing else's text.
    #[test]
    fn drop_empty_removes_all_empty_text_and_preserves_joined_text(nodes in arb_nodes()) {
        let out = collapse(&nodes, CollapseMode::DropEmpty);
        for node in &out {
            if let ContentNode::Text(data) = node {
                prop_assert!(!data.is_empty());
            }
        }
        let before: String = nodes.iter().map(ContentNode::as_str).collect();
        let after: String = out.iter().map(ContentNode::as_str).collect();
        prop_assert_eq!(before, after);
    }

    /// NFC normalization is idempotent.
    #[test]
    fn nfc_normalization_is_idempotent(nodes in arb_nodes()) {
        let once = normalize_unicode(&nodes, &UnicodeNormalizationProfile::Nfc);
        let twice = normalize_unicode(&once, &UnicodeNormalizationProfile::Nfc);
        prop_assert_eq!(once, twice);
    }

    /// Unicode normalization keeps each node's kind and every placeholder's exact
    /// markup; it only ever rewrites text.
    #[test]
    fn normalize_unicode_leaves_placeholders_untouched(nodes in arb_nodes()) {
        let out = normalize_unicode(&nodes, &UnicodeNormalizationProfile::Nfkc);
        prop_assert_eq!(nodes.len(), out.len());
        for (before, after) in nodes.iter().zip(&out) {
            match (before, after) {
                (ContentNode::Placeholder(a), ContentNode::Placeholder(b)) => prop_assert_eq!(a, b),
                (ContentNode::Text(_), ContentNode::Text(_)) => {}
                _ => prop_assert!(false, "node kind changed under normalization"),
            }
        }
    }

    /// Trimming twice is the same as trimming once.
    #[test]
    fn trim_all_both_is_idempotent(nodes in arb_nodes()) {
        let once = trim_nodes(&nodes, TrimMode::TrimAllBoth, &[' ']);
        let twice = trim_nodes(&once, TrimMode::TrimAllBoth, &[' ']);
        prop_assert_eq!(once, twice);
    }

    /// Trimming never alters placeholders (kind, order, or markup).
    #[test]
    fn trim_never_changes_placeholders(nodes in arb_nodes()) {
        let out = trim_nodes(&nodes, TrimMode::TrimAllBoth, &[' ']);
        let placeholders = |ns: &[ContentNode]| -> Vec<String> {
            ns.iter()
                .filter_map(|n| match n {
                    ContentNode::Placeholder(s) => Some(s.clone()),
                    ContentNode::Text(_) => None,
                })
                .collect()
        };
        prop_assert_eq!(placeholders(&nodes), placeholders(&out));
    }

    /// The headline guarantee: once a caller collapses adjacent text, an atom's
    /// id no longer depends on how its text was chunked. Splitting every text run
    /// into single characters and collapsing yields the same id as collapsing the
    /// original.
    #[test]
    fn collapsed_id_is_invariant_to_text_rechunking(nodes in arb_nodes()) {
        let rechunked: Vec<ContentNode> = nodes
            .iter()
            .flat_map(|node| match node {
                ContentNode::Placeholder(_) => vec![node.clone()],
                ContentNode::Text(data) => {
                    data.chars().map(|c| ContentNode::text(c.to_string())).collect()
                }
            })
            .collect();

        let original = en(collapse(&nodes, CollapseMode::CollapseAdjacent));
        let rechunked = en(collapse(&rechunked, CollapseMode::CollapseAdjacent));
        prop_assert_eq!(id_from_atom(&original), id_from_atom(&rechunked));
    }
}
