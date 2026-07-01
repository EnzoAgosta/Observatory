//! Content addressing: turning an [`Observation`] into a stable
//! [`ObservationId`].
//!
//! The id is a SHA-256 digest over a canonical, length-prefixed serialization of
//! the observation *exactly as recorded* — its kind label, its subjects (in
//! order), both bitemporal timestamps, and its payload. Identity is deliberately
//! dumb: it performs no normalization, no subject-order canonicalization, and no
//! semantic collapse. Two observations share an id exactly when they agree on
//! every field, byte for byte.
//!
//! Both timestamps feed the hash, so two recordings of the same fact at different
//! times produce distinct ids — a re-asserted fact is a new event, exactly as the
//! append-only model intends. Subject order is significant, so `[en, fr]` and
//! `[fr, en]` hash differently; whether that order carries a direction is the
//! kind's semantics, not identity's concern. Canonicalizing a symmetric kind
//! (sorting its subjects) is the caller's job, applied before calling
//! [`id_from_observation`] if the caller wants the two orders to collapse.
//!
//! ## Canonical JSON
//!
//! The payload is a [`serde_json::Value`], and JSON object key order is not
//! semantically significant. To keep the id reproducible across serializers (and
//! across `serde_json` feature flags that reorder its internal map), the payload
//! is fed through a small canonicalizer before hashing: object keys are sorted
//! alphabetically at every depth, arrays are left in order (they carry meaning),
//! and the result is emitted compactly. The canonicalizer sorts keys itself
//! rather than relying on `serde_json`'s internal `BTreeMap`, so the output is
//! pinned by this code, not by a feature flag.
//!
//! ## Serialization layout
//!
//! ```text
//! [version: u8]
//! [kind-len: u32 BE][kind-bytes: UTF-8]
//! [subjects-count: u32 BE][subjects: count × 32 bytes, in order]
//! [recorded_at: i64 BE micros]
//! [effective_at: i64 BE micros]
//! [payload-len: u32 BE][payload: canonical JSON UTF-8]
//! ```
//!
//! Lengths are fixed-width big-endian, so ids are identical across platforms, and
//! the framing is self-delimiting. The leading version byte is hashed in-band, so
//! a future scheme can never collide with this one. Subject digests are fixed 32
//! bytes each, so a count prefix (rather than per-element framing) is enough to
//! delimit the list.
//!
//! ## Timestamps
//!
//! [`system_time_to_micros`] converts a [`SystemTime`] to signed microseconds
//! since the Unix epoch — positive after, negative before — and
//! [`micros_to_system_time`] is its inverse. Both are public because the storage
//! layer needs them to encode and decode the Arrow timestamp columns.

use std::time::SystemTime;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::observation::Observation;

/// Version of the canonical serialization scheme, hashed in-band so a future
/// scheme can never collide with this one in `ObservationId` space.
const SERIALIZATION_VERSION: u8 = 0;

/// The content-addressed identity of an [`Observation`]: the 32-byte SHA-256
/// digest of its canonical serialization.
///
/// Two observations share an `ObservationId` exactly when they agree on every
/// field — kind, subjects (in order), both timestamps, and the canonical form of
/// the payload. This is the value to compare and key by; the storage layer
/// derives it itself rather than trusting a caller-supplied one, so a row's key
/// can never disagree with its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationId([u8; 32]);

impl ObservationId {
    /// Wraps a raw 32-byte digest as an `ObservationId` — e.g. one read back
    /// from storage.
    pub fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw 32-byte digest.
    pub fn digest(&self) -> [u8; 32] {
        self.0
    }
}

/// Computes the [`ObservationId`] of `observation`: the SHA-256 of its canonical
/// serialization. Pure in `observation` — the same observation always yields the
/// same id. Both timestamps feed the hash, so two recordings at different times
/// produce distinct ids even if everything else is byte-identical. Subject order
/// is significant, so `[en, fr]` and `[fr, en]` hash differently.
pub fn id_from_observation(observation: &Observation) -> ObservationId {
    let mut hasher = Sha256::new();
    hasher.update([SERIALIZATION_VERSION]);

    update_framed(&mut hasher, observation.kind().as_str().as_bytes());

    let subjects = observation.subjects();
    let count = u32::try_from(subjects.len()).expect("subjects count exceeds u32::MAX");
    hasher.update(count.to_be_bytes());
    for subject in subjects {
        hasher.update(subject.digest());
    }

    hasher.update(system_time_to_micros(observation.recorded_at()).to_be_bytes());
    hasher.update(system_time_to_micros(observation.effective_at()).to_be_bytes());

    update_framed(&mut hasher, &canonical_json_bytes(observation.payload()));

    ObservationId::from_digest(hasher.finalize().into())
}

/// Feeds `bytes` into `hasher` behind a big-endian `u32` length prefix, so field
/// boundaries are unambiguous and adjacent fields cannot be confused for one
/// another.
fn update_framed(hasher: &mut Sha256, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("field exceeds u32::MAX bytes");
    hasher.update(len.to_be_bytes());
    hasher.update(bytes);
}

/// Converts a `SystemTime` to signed microseconds since the Unix epoch. Post-epoch
/// times are positive; pre-epoch times are negative (for backfilled history).
pub fn system_time_to_micros(time: SystemTime) -> i64 {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => {
            i64::try_from(duration.as_micros()).expect("timestamp exceeds i64 micros range")
        }
        Err(error) => {
            let micros = i64::try_from(error.duration().as_micros())
                .expect("pre-epoch timestamp exceeds i64 micros range");
            -micros
        }
    }
}

/// Converts signed microseconds since the Unix epoch back to a `SystemTime`. The
/// inverse of [`system_time_to_micros`]: positive values land after the epoch,
/// negative values before it.
pub fn micros_to_system_time(micros: i64) -> SystemTime {
    if micros >= 0 {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_micros(micros as u64)
    } else {
        SystemTime::UNIX_EPOCH - std::time::Duration::from_micros((-micros) as u64)
    }
}

/// Serializes `value` to canonical JSON bytes: object keys sorted alphabetically
/// at every depth, compact formatting (no insignificant whitespace). Scalars
/// delegate to `serde_json` for escaping and number formatting. We sort keys
/// ourselves rather than relying on `serde_json`'s internal `BTreeMap`, so the
/// output is pinned by this code, not by a feature flag.
fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    let sorted = sort_object_keys(value);
    serde_json::to_vec(&sorted).expect("canonical JSON serialization is infallible")
}

/// Returns a deep copy of `value` with every `Object`'s keys sorted
/// alphabetically. Arrays are left in order (they carry meaning). Scalar values
/// are cloned as-is.
fn sort_object_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), sort_object_keys(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = serde_json::Map::new();
            for (k, v) in entries {
                sorted.insert(k, v);
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_object_keys).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::kind::Kind;
    use observatory_core::identity::{AtomId, id_from_atom};
    use observatory_core::ir::{Atom, ContentNode, LanguageTag};
    use proptest::prelude::*;
    use serde_json::json;

    fn atom_id(lang: &str, text: &str) -> AtomId {
        id_from_atom(&Atom::new(
            LanguageTag::from_string(lang).unwrap(),
            [ContentNode::text(text)],
        ))
    }

    fn kind(label: &str) -> Kind {
        Kind::new(label).unwrap()
    }

    fn observation() -> Observation {
        Observation::relationship(
            kind("translation_of"),
            vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
            SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
            None,
            json!({ "author": "deepl:v2", "confidence": 0.91 }),
        )
    }

    #[test]
    fn id_is_deterministic_for_the_same_observation() {
        let obs = observation();
        assert_eq!(id_from_observation(&obs), id_from_observation(&obs));
    }

    #[test]
    fn different_recorded_at_yields_a_different_id() {
        let anchor = observation();
        let later = Observation::relationship(
            kind("translation_of"),
            vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
            anchor.recorded_at() + Duration::from_secs(1),
            None,
            json!({ "author": "deepl:v2", "confidence": 0.91 }),
        );
        assert_ne!(id_from_observation(&anchor), id_from_observation(&later));
    }

    #[test]
    fn different_effective_at_yields_a_different_id() {
        let anchor = observation();
        let backfilled = Observation::relationship(
            kind("translation_of"),
            vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
            anchor.recorded_at(),
            Some(SystemTime::UNIX_EPOCH),
            json!({ "author": "deepl:v2", "confidence": 0.91 }),
        );
        assert_ne!(id_from_observation(&anchor), id_from_observation(&backfilled));
    }

    #[test]
    fn different_subject_order_yields_a_different_id() {
        let fr = atom_id("fr-FR", "Bonjour");
        let en = atom_id("en-US", "Hello");
        let one = Observation::relationship(
            kind("translation_of"),
            vec![fr, en],
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        let two = Observation::relationship(
            kind("translation_of"),
            vec![en, fr],
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_ne!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn different_kind_yields_a_different_id() {
        let one = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        let two = Observation::property(
            kind("blacklisted"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_ne!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn different_subject_yields_a_different_id() {
        let one = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        let two = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Bonjour"),
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_ne!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn different_payload_yields_a_different_id() {
        let one = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "reviewer": "alice" }),
        );
        let two = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "reviewer": "bob" }),
        );
        assert_ne!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn object_key_order_does_not_affect_the_id() {
        let fr = atom_id("fr-FR", "Bonjour");
        let en = atom_id("en-US", "Hello");
        let one = Observation::relationship(
            kind("translation_of"),
            vec![fr, en],
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "a": 1, "b": 2 }),
        );
        let two = Observation::relationship(
            kind("translation_of"),
            vec![fr, en],
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "b": 2, "a": 1 }),
        );
        assert_eq!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn nested_object_key_order_does_not_affect_the_id() {
        let one = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "outer": { "z": 1, "a": 2 } }),
        );
        let two = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            json!({ "outer": { "a": 2, "z": 1 } }),
        );
        assert_eq!(id_from_observation(&one), id_from_observation(&two));
    }

    #[test]
    fn system_time_to_micros_at_epoch_is_zero() {
        assert_eq!(system_time_to_micros(SystemTime::UNIX_EPOCH), 0);
    }

    #[test]
    fn micros_to_system_time_zero_is_epoch() {
        assert_eq!(micros_to_system_time(0), SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn system_time_to_micros_round_trips_post_epoch() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_micros(1_234_567_890);
        let micros = system_time_to_micros(time);
        assert_eq!(micros, 1_234_567_890);
        assert_eq!(micros_to_system_time(micros), time);
    }

    #[test]
    fn system_time_to_micros_round_trips_pre_epoch() {
        let time = SystemTime::UNIX_EPOCH - Duration::from_micros(1_234_567_890);
        let micros = system_time_to_micros(time);
        assert_eq!(micros, -1_234_567_890);
        assert_eq!(micros_to_system_time(micros), time);
    }

    const HUNDRED_YEARS_MICROS: i64 = 100 * 365 * 24 * 60 * 60 * 1_000_000;

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
                    kind(kind_label),
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
        fn id_is_deterministic_arbitrary(obs in observation_strategy()) {
            prop_assert_eq!(id_from_observation(&obs), id_from_observation(&obs));
        }

        #[test]
        fn system_time_round_trips_arbitrary_within_hundred_years(
            micros in -HUNDRED_YEARS_MICROS..HUNDRED_YEARS_MICROS
        ) {
            let time = micros_to_system_time(micros);
            prop_assert_eq!(system_time_to_micros(time), micros);
        }
    }
}
