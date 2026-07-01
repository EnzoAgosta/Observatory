//! Property-based round-trip fidelity for the Arrow layer.
//!
//! The headline invariant: for any batch of atoms, decoding what we encoded yields
//! every *distinct* atom that went in, in first-seen order — over arbitrary content
//! (any Unicode, empty strings, data colliding with the `node_kind` tag words) and
//! varying arities. "Distinct" because encoding collapses byte-identical atoms (the
//! `row_digest` dedup), so a duplicated input round-trips to a single row.

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
    fn round_trips_distinct_atoms(atoms in prop::collection::vec(atom_strategy(), 0..16)) {
        // Encoding dedups byte-identical atoms, so the faithful expectation is the
        // input with duplicates dropped in first-seen order, not the raw input.
        let mut expected: Vec<Atom> = Vec::new();
        for atom in &atoms {
            if !expected.contains(atom) {
                expected.push(atom.clone());
            }
        }
        let batch = encode_atoms(&atoms);
        prop_assert_eq!(decode_atoms(&batch).unwrap(), expected);
    }
}
