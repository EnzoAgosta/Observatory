//! Encoding atoms and observations into Arrow `RecordBatch`es shaped like their
//! schemas.
//!
//! Pure and infallible: both encoders are handed already-valid domain types and
//! compute the content-addressed keys themselves (the matching `AtomId` and the
//! exact `row_digest` for atoms; the `ObservationId` for observations), so nothing
//! here can fail; the `expect`s document invariants that hold by construction (a
//! digest is always 32 bytes; the field builders are the types we just built).
//! Byte-identical items within one call collapse to a single row — deduped by
//! `row_digest` for atoms, by `observation_id` for observations — so the merge key
//! is never duplicated inside a batch. Batch-first: one call produces one
//! `RecordBatch` (one Lance commit, later), never a per-item write — see
//! `DESIGN.md`.

use std::collections::HashSet;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, FixedSizeBinaryBuilder, ListBuilder, RecordBatch, StringBuilder, StructBuilder,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field};

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode};
use observatory_observations::identity::id_from_observation;
use observatory_observations::identity::system_time_to_micros;
use observatory_observations::Observation;
use sha2::{Digest, Sha256};

use crate::schema::{
    DIGEST_WIDTH, NODE, NODE_KIND_PLACEHOLDER, NODE_KIND_TEXT, SUBJECT_FIELD, atoms_schema,
    content_node_fields, observations_schema,
};

/// Encodes `atoms` into a single `RecordBatch` matching the atoms schema. Each
/// atom's `AtomId` is derived here, never taken from the caller.
pub fn encode_atoms(atoms: &[Atom]) -> RecordBatch {
    let mut atom_id_column = FixedSizeBinaryBuilder::new(DIGEST_WIDTH);
    let mut row_digest_column = FixedSizeBinaryBuilder::new(DIGEST_WIDTH);
    let mut language_column = StringBuilder::new();

    let node_field = Arc::new(Field::new(
        NODE,
        DataType::Struct(content_node_fields()),
        false,
    ));
    let struct_builder = StructBuilder::from_fields(content_node_fields(), 0);
    let mut content_column = ListBuilder::new(struct_builder).with_field(node_field);

    let mut seen = HashSet::new();
    for atom in atoms {
        let digest = row_digest(atom);
        if !seen.insert(digest) {
            continue; // a byte-identical atom is already in this batch
        }
        encode_atom(
            atom,
            digest,
            &mut atom_id_column,
            &mut row_digest_column,
            &mut language_column,
            &mut content_column,
        );
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(atom_id_column.finish()),
        Arc::new(row_digest_column.finish()),
        Arc::new(language_column.finish()),
        Arc::new(content_column.finish()),
    ];

    RecordBatch::try_new(atoms_schema(), columns)
        .expect("encoded arrays match the atoms schema by construction")
}

/// Appends one atom across all column builders in lockstep — its two keys, its
/// language, and its content nodes — so a partially-written atom is impossible.
/// The `row_digest` is passed in (already computed for the batch dedup) so it is
/// hashed once per atom, not twice.
fn encode_atom(
    atom: &Atom,
    digest: [u8; 32],
    atom_id_column: &mut FixedSizeBinaryBuilder,
    row_digest_column: &mut FixedSizeBinaryBuilder,
    language_column: &mut StringBuilder,
    content_column: &mut ListBuilder<StructBuilder>,
) {
    let id = id_from_atom(atom);
    atom_id_column
        .append_value(id.digest())
        .expect("an AtomId digest is exactly DIGEST_WIDTH bytes");
    row_digest_column
        .append_value(digest)
        .expect("a row_digest is exactly DIGEST_WIDTH bytes");
    language_column.append_value(atom.language().as_str());

    let nodes = content_column.values();
    for node in atom.content() {
        let (kind, data) = match node {
            ContentNode::Text(data) => (NODE_KIND_TEXT, data),
            ContentNode::Placeholder(data) => (NODE_KIND_PLACEHOLDER, data),
        };
        nodes
            .field_builder::<StringBuilder>(0)
            .expect("content struct field 0 is the node_kind StringBuilder")
            .append_value(kind);
        nodes
            .field_builder::<StringBuilder>(1)
            .expect("content struct field 1 is the data StringBuilder")
            .append_value(data);
        nodes.append(true);
    }
    content_column.append(true);
}

/// Version of the `row_digest` serialization below. Bump it if the framing ever
/// changes: persisted digests are only comparable within one version, so a change
/// makes old-format rows recognizably distinct rather than silently colliding.
const ROW_DIGEST_VERSION: u8 = 1;

/// Per-node variant tags in the `row_digest` serialization — the byte that keeps a
/// text run distinct from a placeholder holding the same data.
const ROW_DIGEST_TAG_TEXT: u8 = 0;
const ROW_DIGEST_TAG_PLACEHOLDER: u8 = 1;

/// The exact-identity digest of `atom`: a SHA-256 over a length-framed
/// serialization of its language and *full* content — placeholder markup included,
/// unlike [`AtomId`](observatory_core::identity::AtomId). The framing (a length
/// prefix on every field and a variant tag on every node) makes the serialization
/// injective, so two distinct atoms can never hash to the same digest and be wrongly
/// deduped. Applies no normalization by design: `en-US` and `en-us` are different
/// rows, as are two atoms differing only in a placeholder's markup.
fn row_digest(atom: &Atom) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([ROW_DIGEST_VERSION]);
    update_framed(&mut hasher, atom.language().as_str().as_bytes());
    for node in atom.content() {
        let (tag, data) = match node {
            ContentNode::Text(data) => (ROW_DIGEST_TAG_TEXT, data),
            ContentNode::Placeholder(data) => (ROW_DIGEST_TAG_PLACEHOLDER, data),
        };
        hasher.update([tag]);
        update_framed(&mut hasher, data.as_bytes());
    }
    hasher.finalize().into()
}

/// Feeds `bytes` into `hasher` behind a big-endian `u32` length prefix, so field
/// boundaries are unambiguous and adjacent fields cannot be confused for one
/// another (the property that makes `row_digest` injective).
fn update_framed(hasher: &mut Sha256, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("a content field exceeds u32::MAX bytes");
    hasher.update(len.to_be_bytes());
    hasher.update(bytes);
}

/// Encodes `observations` into a single `RecordBatch` matching the observations
/// schema. Each observation's `ObservationId` is derived here via
/// [`id_from_observation`], never taken from the caller. Byte-identical
/// observations within one call collapse to a single row (deduped by id), so the
/// merge key is never duplicated inside a batch.
pub fn encode_observations(observations: &[Observation]) -> RecordBatch {
    let mut columns = ObservationColumns::new();
    let mut seen = HashSet::new();
    for obs in observations {
        let digest = id_from_observation(obs).digest();
        if !seen.insert(digest) {
            continue;
        }
        columns.append(obs, digest);
    }
    columns.finish()
}

/// The per-column builders for one observations batch, bundled so that appending
/// one observation across all columns stays atomic (a partially-written row is
/// impossible) and [`finish`](Self::finish) assembles the `RecordBatch` in one
/// place that matches the observations schema by construction.
struct ObservationColumns {
    observation_id: FixedSizeBinaryBuilder,
    kind: StringBuilder,
    subjects: ListBuilder<FixedSizeBinaryBuilder>,
    recorded_at: Vec<i64>,
    effective_at: Vec<i64>,
    payload: StringBuilder,
}

impl ObservationColumns {
    fn new() -> Self {
        let subject_field = Arc::new(Field::new(
            SUBJECT_FIELD,
            DataType::FixedSizeBinary(DIGEST_WIDTH),
            false,
        ));
        Self {
            observation_id: FixedSizeBinaryBuilder::new(DIGEST_WIDTH),
            kind: StringBuilder::new(),
            subjects: ListBuilder::new(FixedSizeBinaryBuilder::new(DIGEST_WIDTH))
                .with_field(subject_field),
            recorded_at: Vec::new(),
            effective_at: Vec::new(),
            payload: StringBuilder::new(),
        }
    }

    /// Appends `obs` across all column builders in lockstep — its id, kind,
    /// subjects, both timestamps, and payload — so a partially-written
    /// observation is impossible. The `digest` is passed in (already computed
    /// for the batch dedup) so it is hashed once per observation, not twice.
    fn append(&mut self, obs: &Observation, digest: [u8; 32]) {
        self.observation_id
            .append_value(digest)
            .expect("an ObservationId digest is exactly DIGEST_WIDTH bytes");
        self.kind.append_value(obs.kind().as_str());

        let subject_builder = self.subjects.values();
        for subject in obs.subjects() {
            subject_builder
                .append_value(subject.digest())
                .expect("an AtomId digest is exactly DIGEST_WIDTH bytes");
        }
        self.subjects.append(true);

        self.recorded_at
            .push(system_time_to_micros(obs.recorded_at()));
        self.effective_at
            .push(system_time_to_micros(obs.effective_at()));

        let payload = serde_json::to_vec(obs.payload()).expect("JSON serialization is infallible");
        let payload_str = std::str::from_utf8(&payload).expect("JSON is valid UTF-8");
        self.payload.append_value(payload_str);
    }

    /// Finishes all builders and assembles the `RecordBatch` in observations
    /// schema order.
    fn finish(mut self) -> RecordBatch {
        let recorded_at =
            TimestampMicrosecondArray::from(self.recorded_at).with_timezone("UTC");
        let effective_at =
            TimestampMicrosecondArray::from(self.effective_at).with_timezone("UTC");

        let columns: Vec<ArrayRef> = vec![
            Arc::new(self.observation_id.finish()),
            Arc::new(self.kind.finish()),
            Arc::new(self.subjects.finish()),
            Arc::new(recorded_at),
            Arc::new(effective_at),
            Arc::new(self.payload.finish()),
        ];

        RecordBatch::try_new(observations_schema(), columns)
            .expect("encoded arrays match the observations schema by construction")
    }
}
