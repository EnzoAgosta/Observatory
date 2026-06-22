//! Property-based invariants for the content-addressing pipeline.
//!
//! These check the load-bearing guarantees over randomly generated inputs:
//! collapsing is idempotent and lossless, normalization is idempotent, and an
//! atom's identity does not depend on how its text happened to be split into
//! runs.

use observatory::identity::{atom_id, collapse};
use observatory::ir::{Atom, ContentNode, LanguageTag};
use observatory::normalize::{NormalizationProfile, normalize_content};
use proptest::prelude::*;

fn text(data: String) -> ContentNode {
    ContentNode::text(data)
}

fn placeholder(data: String) -> ContentNode {
    ContentNode::placeholder(data)
}

fn english() -> LanguageTag {
    LanguageTag::parse("en-us").unwrap()
}

fn arb_node() -> impl Strategy<Value = ContentNode> {
    prop_oneof![
        any::<String>().prop_map(text),
        any::<String>().prop_map(placeholder),
    ]
}

fn arb_nodes() -> impl Strategy<Value = Vec<ContentNode>> {
    prop::collection::vec(arb_node(), 0..12)
}

proptest! {
    /// Collapsing twice is the same as collapsing once.
    #[test]
    fn collapse_is_idempotent(nodes in arb_nodes()) {
        let once = collapse(&nodes);
        prop_assert_eq!(collapse(&once), once);
    }

    /// Collapsing never changes the string an atom reconstructs to.
    #[test]
    fn collapse_preserves_joined_text(nodes in arb_nodes()) {
        let before: String = nodes.iter().map(ContentNode::data).collect();
        let after: String = collapse(&nodes).iter().map(ContentNode::data).collect();
        prop_assert_eq!(before, after);
    }

    /// Collapsed output is canonical: no empty text run, and never two text runs
    /// in a row.
    #[test]
    fn collapse_yields_canonical_structure(nodes in arb_nodes()) {
        let collapsed = collapse(&nodes);
        for node in &collapsed {
            if !node.is_placeholder() {
                prop_assert!(!node.data().is_empty());
            }
        }
        for pair in collapsed.windows(2) {
            prop_assert!(pair[0].is_placeholder() || pair[1].is_placeholder());
        }
    }

    /// Normalizing twice is the same as normalizing once.
    #[test]
    fn normalize_is_idempotent(nodes in arb_nodes()) {
        let profile = NormalizationProfile::default();
        let once = normalize_content(&collapse(&nodes), &profile);
        let twice = normalize_content(&once, &profile);
        prop_assert_eq!(twice, once);
    }

    /// An atom's identity is independent of how its text was split into runs:
    /// many text chunks and the single concatenated string hash alike.
    #[test]
    fn atom_id_is_invariant_to_text_chunking(chunks in prop::collection::vec(any::<String>(), 0..8)) {
        let profile = NormalizationProfile::default();
        let chunked = Atom::new(english(), chunks.iter().cloned().map(text));
        let single = Atom::new(english(), [text(chunks.concat())]);
        prop_assert_eq!(atom_id(&chunked, &profile), atom_id(&single, &profile));
    }
}
