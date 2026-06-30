//! An [`Observation`]: an append-only fact about one atom or among several.

use std::time::SystemTime;

use observatory_core::identity::AtomId;
use serde_json::Value;

use crate::id::ObservationId;
use crate::kind::Kind;

/// An append-only fact about one atom (a *property*) or among several
/// (a *relationship*), keyed to its subjects by [`AtomId`].
///
/// Construction is faithful: [`property`](Self::property) and
/// [`relationship`](Self::relationship) record exactly the subjects they are
/// given and judge nothing about them — two intent-named entry points to the same
/// shape, not a type-enforced split. The distinction is a hint to the reader;
/// nothing prevents a one-subject relationship, and sharper constructors or
/// deeper validation are conveniences to add later, not policy imposed here.
///
/// Subject order is *preserved* exactly as given, but the crate ascribes it no
/// meaning. Whether order is a *direction* (source→target) or noise is the kind's
/// semantics — a translation is a symmetric equivalence (`en-US` ⇄ `fr-FR`,
/// neither side privileged), while a kind like "context for" reads its order — and
/// the crate enforces neither. Two recordings that differ only in subject order
/// are therefore distinct observations here; collapsing them into one fact (and
/// any other deduplication) is the caller's job, the same way `observatory-core`
/// records atoms faithfully and leaves dedup-by-`AtomId` to whoever stores them.
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
    /// The shared, faithful constructor both public entry points delegate to:
    /// records the fields exactly as given, once `effective_at` has been
    /// resolved.
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

    /// A **property**: a fact about a single atom — reviewed, blacklisted, a
    /// domain assignment, and so on.
    ///
    /// `effective_at` defaults to `recorded_at` when `None`.
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

    /// A **relationship**: a fact among atoms — a translation, a context link, an
    /// equivalence. Subject order is significant; what it *means* (a direction, or
    /// nothing) is the kind's semantics, the caller's to define.
    ///
    /// `effective_at` defaults to `recorded_at` when `None`.
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

    /// The observation's minted event id.
    pub fn id(&self) -> ObservationId {
        self.id
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

