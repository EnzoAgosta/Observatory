use std::time::SystemTime;

use observatory_core::identity::AtomId;
use serde_json::Value;

use crate::id::ObservationId;
use crate::kind::Kind;

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    id: ObservationId,
    kind: Kind,
    subjects: Vec<AtomId>,
    recorded_at: SystemTime,
    effective_at: SystemTime,
    payload: Value,
}

impl Observation {
    fn new(
        id: ObservationId,
        kind: Kind,
        subjects: Vec<AtomId>,
        recorded_at: SystemTime,
        effective_at: SystemTime,
        payload: Value,
    ) -> Self {
        Self {
            id,
            kind,
            subjects,
            recorded_at,
            effective_at,
            payload,
        }
    }

    pub fn property(
        id: ObservationId,
        kind: Kind,
        subject: AtomId,
        recorded_at: SystemTime,
        effective_at: Option<SystemTime>,
        payload: Value,
    ) -> Self {
        Self::new(
            id,
            kind,
            vec![subject],
            recorded_at,
            effective_at.unwrap_or(recorded_at),
            payload,
        )
    }

    pub fn relationship(
        id: ObservationId,
        kind: Kind,
        subjects: Vec<AtomId>,
        recorded_at: SystemTime,
        effective_at: Option<SystemTime>,
        payload: Value,
    ) -> Self {
        Self::new(
            id,
            kind,
            subjects,
            recorded_at,
            effective_at.unwrap_or(recorded_at),
            payload,
        )
    }

    pub fn id(&self) -> ObservationId {
        self.id
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn subjects(&self) -> &[AtomId] {
        &self.subjects
    }

    pub fn recorded_at(&self) -> SystemTime {
        self.recorded_at
    }

    pub fn effective_at(&self) -> SystemTime {
        self.effective_at
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

