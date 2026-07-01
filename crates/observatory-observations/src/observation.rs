//! An [`Observation`]: an append-only fact about one atom or among several.

use std::time::SystemTime;

use observatory_core::identity::AtomId;
use serde_json::Value;

use crate::kind::Kind;

/// An append-only fact about one atom (a *property*) or among several
/// (a *relationship*), keyed to its subjects by [`AtomId`]. The observation's
/// identity is *content-derived*: see [`id_from_observation`](crate::id_from_observation).
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    kind: Kind,
    subjects: Vec<AtomId>,
    recorded_at: SystemTime,
    effective_at: SystemTime,
    payload: Value,
}

impl Observation {
    /// The faithful constructor: records the fields exactly as given, with no
    /// defaults applied. The storage layer uses this to round-trip a stored
    /// observation without re-defaulting `effective_at`; callers usually want
    /// [`property`](Self::property) or [`relationship`](Self::relationship)
    /// instead.
    pub fn new(
        kind: Kind,
        subjects: Vec<AtomId>,
        recorded_at: SystemTime,
        effective_at: SystemTime,
        payload: Value,
    ) -> Self {
        Self {
            kind,
            subjects,
            recorded_at,
            effective_at,
            payload,
        }
    }

    /// A **property**: a fact about a single atom — reviewed, blacklisted, a
    /// domain assignment, and so on.
    ///
    /// `effective_at` defaults to `recorded_at` when `None`.
    pub fn property(
        kind: Kind,
        subject: AtomId,
        recorded_at: SystemTime,
        effective_at: Option<SystemTime>,
        payload: Value,
    ) -> Self {
        Self::new(
            kind,
            vec![subject],
            recorded_at,
            effective_at.unwrap_or(recorded_at),
            payload,
        )
    }

    /// A **relationship**: a fact among atoms — a translation, a context link, an
    /// equivalence. Subject order is significant; what it *means* (a direction, or
    /// nothing) is the kind's semantics, the caller's to define.
    ///
    /// `effective_at` defaults to `recorded_at` when `None`.
    pub fn relationship(
        kind: Kind,
        subjects: Vec<AtomId>,
        recorded_at: SystemTime,
        effective_at: Option<SystemTime>,
        payload: Value,
    ) -> Self {
        Self::new(
            kind,
            subjects,
            recorded_at,
            effective_at.unwrap_or(recorded_at),
            payload,
        )
    }

    /// What the observation asserts.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// The atoms this observation is keyed to, in order — typically one for a
    /// property and two or more for a relationship, though neither is enforced.
    /// The order carries whatever the kind's semantics give it.
    pub fn subjects(&self) -> &[AtomId] {
        &self.subjects
    }

    /// When the fact was recorded (transaction time).
    pub fn recorded_at(&self) -> SystemTime {
        self.recorded_at
    }

    /// When the fact became true in the world (valid time); equals
    /// [`recorded_at`](Self::recorded_at) unless a distinct time was supplied.
    pub fn effective_at(&self) -> SystemTime {
        self.effective_at
    }

    /// The kind-specific payload — provenance, scores, reasons, and anything else
    /// the envelope does not first-class.
    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_core::identity::id_from_atom;
    use observatory_core::ir::{Atom, ContentNode, LanguageTag};

    fn atom_id(lang: &str, text: &str) -> AtomId {
        id_from_atom(&Atom::new(
            LanguageTag::from_string(lang).unwrap(),
            [ContentNode::text(text)],
        ))
    }

    fn kind(label: &str) -> Kind {
        Kind::new(label).unwrap()
    }

    #[test]
    fn property_keys_to_a_single_subject() {
        let subject = atom_id("en-US", "Hello");
        let obs = Observation::property(
            kind("blacklisted"),
            subject,
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_eq!(obs.subjects(), &[subject]);
    }

    #[test]
    fn relationship_keeps_its_subjects_in_order() {
        let fr = atom_id("fr-FR", "Bonjour");
        let en = atom_id("en-US", "Hello");
        let obs = Observation::relationship(
            kind("translation_of"),
            vec![fr, en],
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_eq!(obs.subjects(), &[fr, en]);
    }

    #[test]
    fn relationship_records_a_single_subject_faithfully() {
        let only = atom_id("en-US", "lonely");
        let obs = Observation::relationship(
            kind("interchangeable"),
            vec![only],
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_eq!(obs.subjects(), &[only]);
    }

    #[test]
    fn effective_at_defaults_to_recorded_at() {
        let recorded = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        let obs = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            recorded,
            None,
            Value::Null,
        );
        assert_eq!(obs.effective_at(), recorded);
    }

    #[test]
    fn effective_at_can_differ_from_recorded_at() {
        let effective = SystemTime::UNIX_EPOCH;
        let recorded = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86400);
        let obs = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            recorded,
            Some(effective),
            Value::Null,
        );
        assert_eq!(obs.recorded_at(), recorded);
        assert_eq!(obs.effective_at(), effective);
    }

    #[test]
    fn new_does_not_apply_default_effective_at() {
        let recorded = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        let effective = SystemTime::UNIX_EPOCH;
        let obs = Observation::new(
            kind("approved_by"),
            vec![atom_id("en-US", "Hello")],
            recorded,
            effective,
            Value::Null,
        );
        assert_eq!(obs.recorded_at(), recorded);
        assert_eq!(obs.effective_at(), effective);
    }

    #[test]
    fn kind_is_preserved_verbatim() {
        let obs = Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            None,
            Value::Null,
        );
        assert_eq!(obs.kind().as_str(), "approved_by");
    }

    #[test]
    fn payload_is_preserved() {
        let payload = serde_json::json!({ "confidence": 0.91, "author": "deepl:v2" });
        let obs = Observation::relationship(
            kind("translation_of"),
            vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
            SystemTime::UNIX_EPOCH,
            None,
            payload.clone(),
        );
        assert_eq!(obs.payload(), &payload);
    }
}
