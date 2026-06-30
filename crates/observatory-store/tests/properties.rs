//! Property-based round-trip fidelity for the Arrow layer.
//!
//! The headline invariant: for any batch of atoms, decoding what we encoded
//! yields exactly what went in — over arbitrary content (any Unicode, empty
//! strings, data colliding with the `node_kind` tag words) and varying arities.

use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_store::{decode_atoms, encode_atoms};
use proptest::prelude::*;

fn content_node_strategy() -> impl Strategy<Value = ContentNode> {
    prop_oneof![
        any::<String>().prop_map(ContentNode::Text),
        any::<String>().prop_map(ContentNode::Placeholder),
    ]
}

fn atom_strategy() -> impl Strategy<Value = Atom> {
    let languages = prop::sample::select(vec![
        "en-US", "fr-FR", "de-DE", "ja-JP", "pt-BR", "zh-CN", "es-ES", "en-GB",
    ]);
    (
        languages,
        prop::collection::vec(content_node_strategy(), 0..8),
    )
        .prop_map(|(language, nodes)| Atom::new(LanguageTag::from_string(language).unwrap(), nodes))
}

proptest! {
    #[test]
    fn round_trips_arbitrary_atoms(atoms in prop::collection::vec(atom_strategy(), 0..16)) {
        let batch = encode_atoms(&atoms);
        prop_assert_eq!(decode_atoms(&batch).unwrap(), atoms);
    }
}
