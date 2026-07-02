//! Property-based round-trip fidelity for the Arrow layer.
//!
//! The headline invariant: for any batch of atoms, decoding what we encoded yields
//! every *distinct* atom that went in, in first-seen order — over arbitrary content
//! (any Unicode, empty strings, data colliding with the `node_kind` tag words) and
//! varying arities. "Distinct" because encoding collapses byte-identical atoms (the
//! `row_digest` dedup), so a duplicated input round-trips to a single row.

use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_lance::{decode_atoms, decode_observations, encode_atoms, encode_observations};
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

// --- observations ---

use std::time::SystemTime;

use observatory_core::identity::AtomId;
use observatory_observations::{Kind, Observation, id_from_observation, identity::micros_to_system_time};
use serde_json::Value;

fn json_strategy() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<String>().prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 16, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec((any::<String>(), inner), 0..4)
                .prop_map(|pairs| Value::Object(pairs.into_iter().collect())),
        ]
    })
}

const HUNDRED_YEARS_MICROS: i64 = 100 * 365 * 24 * 60 * 60 * 1_000_000;

fn system_time_strategy() -> impl Strategy<Value = SystemTime> {
    (-HUNDRED_YEARS_MICROS..HUNDRED_YEARS_MICROS).prop_map(micros_to_system_time)
}

fn observation_strategy() -> impl Strategy<Value = Observation> {
    let kinds = prop::sample::select(vec![
        "translation_of",
        "approved_by",
        "blacklisted",
        "context_for",
        "interchangeable",
    ]);
    let subjects = prop::collection::vec(any::<[u8; 32]>(), 1..4);
    (kinds, subjects, system_time_strategy(), json_strategy()).prop_map(
        |(kind_label, subjects, recorded_at, payload)| {
            Observation::new(
                Kind::new(kind_label).unwrap(),
                subjects.into_iter().map(AtomId::from_digest).collect(),
                recorded_at,
                recorded_at,
                payload,
            )
        },
    )
}

proptest! {
    #[test]
    fn round_trips_distinct_observations(
        observations in prop::collection::vec(observation_strategy(), 0..16)
    ) {
        let mut expected: Vec<Observation> = Vec::new();
        for obs in &observations {
            let id = id_from_observation(obs);
            if !expected.iter().any(|o| id_from_observation(o) == id) {
                expected.push(obs.clone());
            }
        }

        let batch = encode_observations(&observations);
        prop_assert_eq!(decode_observations(&batch).unwrap(), expected);
    }
}
