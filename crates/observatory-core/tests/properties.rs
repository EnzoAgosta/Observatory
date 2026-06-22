//! Property-based invariants for the content-addressing pipeline.
//!
//! These check the load-bearing guarantees over randomly generated inputs:
//! collapsing is idempotent and lossless, normalization is idempotent, and an
//! atom's identity does not depend on how its text happened to be split into
//! runs.

use observatory_core::identity::{atom_id, canonical_bytes, collapse};
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_core::normalize::{NormalizationProfile, normalize_content};
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

    /// Normalized output stays canonical: no empty text run and never two text
    /// runs adjacent. The serialization assumes this, and the NFKC-before-trim
    /// order above is what preserves it (NFKC never empties a non-empty run, and
    /// trim only shortens or drops the first/last node, so it can't create
    /// adjacency from collapsed input).
    #[test]
    fn normalize_yields_canonical_structure(nodes in arb_nodes()) {
        let profile = NormalizationProfile::default();
        let out = normalize_content(&collapse(&nodes), &profile);
        for node in &out {
            if !node.is_placeholder() {
                prop_assert!(!node.data().is_empty());
            }
        }
        for pair in out.windows(2) {
            prop_assert!(pair[0].is_placeholder() || pair[1].is_placeholder());
        }
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

    /// Re-chunking text *around* placeholders does not change the identity:
    /// splitting every text run into single characters, with placeholders left in
    /// place, yields the same `AtomId`.
    #[test]
    fn atom_id_is_invariant_to_text_rechunking_around_placeholders(nodes in arb_nodes()) {
        let profile = NormalizationProfile::default();
        let original = Atom::new(english(), nodes.clone());
        let rechunked: Vec<ContentNode> = nodes
            .iter()
            .flat_map(|node| {
                if node.is_placeholder() {
                    vec![node.clone()]
                } else {
                    node.data().chars().map(|c| text(c.to_string())).collect()
                }
            })
            .collect();
        let rechunked = Atom::new(english(), rechunked);
        prop_assert_eq!(atom_id(&original, &profile), atom_id(&rechunked, &profile));
    }

    /// The serialization is injective on the identity-relevant projection: two
    /// atoms produce the same canonical bytes exactly when their normalized text
    /// and placeholder positions match. Placeholder *markup* is excluded from
    /// identity, so it is dropped from the projection too. This guards against a
    /// future framing change silently introducing a collision (different
    /// content, same id).
    #[test]
    fn canonical_bytes_are_injective_on_identity_projection(a in arb_nodes(), b in arb_nodes()) {
        let profile = NormalizationProfile::default();
        let atom_a = Atom::new(english(), a);
        let atom_b = Atom::new(english(), b);
        let key_a = identity_projection(&normalize_content(&collapse(atom_a.content()), &profile));
        let key_b = identity_projection(&normalize_content(&collapse(atom_b.content()), &profile));
        prop_assert_eq!(
            key_a == key_b,
            canonical_bytes(&atom_a, &profile) == canonical_bytes(&atom_b, &profile)
        );
    }
}

/// The part of a node sequence that identity actually encodes: each text run by
/// its content, each placeholder by mere presence (its markup is excluded).
fn identity_projection(nodes: &[ContentNode]) -> Vec<Option<String>> {
    nodes
        .iter()
        .map(|node| (!node.is_placeholder()).then(|| node.data().to_owned()))
        .collect()
}
