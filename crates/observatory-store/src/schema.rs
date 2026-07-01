//! The Arrow schemas for the **atoms** and **observations** tables — the single
//! source of truth for their on-disk column layouts, and the wire-format
//! vocabulary (column names and the `node_kind` tag values) that `encode` and
//! `decode` share.
//!
//! ## Atoms table
//!
//! | column       | type                                        |
//! |--------------|---------------------------------------------|
//! | `atom_id`    | `FixedSizeBinary(32)`                       |
//! | `row_digest` | `FixedSizeBinary(32)`                       |
//! | `language`   | `Utf8`                                      |
//! | `content`    | `List<Struct<node_kind: Utf8, data: Utf8>>` |
//!
//! The table carries **two keys with two jobs**. `atom_id` is the content-addressed
//! *matching* key — a SHA-256 that, by design, excludes placeholder markup — so it
//! is deliberately lossy and **not unique**: two atoms differing only in placeholder
//! markup share it. `row_digest` is the internal *exact* key — a SHA-256 over the
//! full atom, markup included — used solely as the upsert/dedup key so that
//! re-storing a byte-identical atom is a no-op while genuine markup variants are
//! kept apart. It is an implementation detail: callers never supply or query it, and
//! `decode` ignores it (it reconstructs an atom from `language` + `content` alone).
//!
//! Every field is non-nullable: the model has no null node or field, and an atom
//! with no content is an *empty* list, never a null one. A content node is a
//! struct tagged by `node_kind` (`"text"` or `"placeholder"`) rather than an Arrow
//! `Union`, because Lance 7.0.0 cannot store a `Union` (see `DESIGN.md`).
//!
//! ## Observations table
//!
//! | column           | type                      |
//! |------------------|---------------------------|
//! | `observation_id` | `FixedSizeBinary(32)`       |
//! | `kind`           | `Utf8`                      |
//! | `subjects`       | `List<FixedSizeBinary(32)>` |
//! | `recorded_at`    | `Timestamp(Microsecond, "UTC")` |
//! | `effective_at`   | `Timestamp(Microsecond, "UTC")` |
//! | `payload`        | `Utf8`                      |
//!
//! Unlike the atoms table, the observations table carries **one key, not two**.
//! `observation_id` is the content-addressed identity — a SHA-256 over the
//! observation's canonical serialization (kind, subjects in order, both timestamps,
//! and the canonical JSON of the payload) — computed by
//! [`id_from_observation`](observatory_observations::id_from_observation). It is
//! both the matching key and the exact dedup key: there is no "lossy matching" vs
//! "exact identity" split, because observations have no analog to the atom's
//! placeholder-markup-exclusion. The store derives it itself rather than trusting a
//! caller-supplied one, so a row's key can never disagree with its content.
//!
//! Every field is non-nullable. `subjects` is a list of fixed-width digests (one
//! per `AtomId`), in the order the observation was given — order is significant and
//! preserved. The two timestamps are signed micros since the Unix epoch
//! (`Timestamp(Microsecond, "UTC")`), so pre-epoch backfilled history works. The
//! payload is the `serde_json::Value` serialized to a JSON string — DuckDB parses it
//! with JSON functions later, and the store never interprets it.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef, TimeUnit};

/// Byte width of a SHA-256 digest; both `atom_id` and `row_digest` are one, and
/// Arrow's `FixedSizeBinary` takes an `i32`.
pub(crate) const DIGEST_WIDTH: i32 = 32;

pub(crate) const ATOM_ID_COLUMN: &str = "atom_id";
pub(crate) const ROW_DIGEST_COLUMN: &str = "row_digest";
pub(crate) const LANGUAGE_COLUMN: &str = "language";
pub(crate) const CONTENT_NODES: &str = "content";

/// Name Arrow gives a list's element field; here, one content node.
pub(crate) const NODE: &str = "node";
pub(crate) const NODE_KIND_FIELD: &str = "node_kind";
pub(crate) const NODE_DATA_FIELD: &str = "data";

/// `node_kind` tag for a text content node.
pub(crate) const NODE_KIND_TEXT: &str = "text";
/// `node_kind` tag for a placeholder content node.
pub(crate) const NODE_KIND_PLACEHOLDER: &str = "placeholder";

/// The fields of one content node — `node_kind` and `data`, both non-null `Utf8`.
/// Shared by the schema and the encoder so their struct types cannot drift.
pub(crate) fn content_node_fields() -> Fields {
    Fields::from(vec![
        Field::new(NODE_KIND_FIELD, DataType::Utf8, false),
        Field::new(NODE_DATA_FIELD, DataType::Utf8, false),
    ])
}

/// The Arrow schema of the atoms table (see the module docs).
pub(crate) fn atoms_schema() -> SchemaRef {
    let node_field = Field::new(NODE, DataType::Struct(content_node_fields()), false);
    Arc::new(Schema::new(vec![
        Field::new(
            ATOM_ID_COLUMN,
            DataType::FixedSizeBinary(DIGEST_WIDTH),
            false,
        ),
        Field::new(
            ROW_DIGEST_COLUMN,
            DataType::FixedSizeBinary(DIGEST_WIDTH),
            false,
        ),
        Field::new(LANGUAGE_COLUMN, DataType::Utf8, false),
        Field::new(CONTENT_NODES, DataType::List(Arc::new(node_field)), false),
    ]))
}

pub(crate) const OBSERVATION_ID_COLUMN: &str = "observation_id";
pub(crate) const KIND_COLUMN: &str = "kind";
pub(crate) const SUBJECTS_COLUMN: &str = "subjects";
pub(crate) const RECORDED_AT_COLUMN: &str = "recorded_at";
pub(crate) const EFFECTIVE_AT_COLUMN: &str = "effective_at";
pub(crate) const PAYLOAD_COLUMN: &str = "payload";

/// Name Arrow gives the subjects list's element field; here, one `AtomId`.
pub(crate) const SUBJECT_FIELD: &str = "atom_id";

/// `Timestamp(Microsecond, "UTC")` — shared by `recorded_at` and `effective_at`.
/// A signed `i64` of micros since the Unix epoch, with the timezone as metadata.
fn utc_timestamp() -> DataType {
    DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC")))
}

/// The Arrow schema of the observations table (see the module docs above).
pub(crate) fn observations_schema() -> SchemaRef {
    let subject_field = Field::new(
        SUBJECT_FIELD,
        DataType::FixedSizeBinary(DIGEST_WIDTH),
        false,
    );
    Arc::new(Schema::new(vec![
        Field::new(
            OBSERVATION_ID_COLUMN,
            DataType::FixedSizeBinary(DIGEST_WIDTH),
            false,
        ),
        Field::new(KIND_COLUMN, DataType::Utf8, false),
        Field::new(
            SUBJECTS_COLUMN,
            DataType::List(Arc::new(subject_field)),
            false,
        ),
        Field::new(RECORDED_AT_COLUMN, utc_timestamp(), false),
        Field::new(EFFECTIVE_AT_COLUMN, utc_timestamp(), false),
        Field::new(PAYLOAD_COLUMN, DataType::Utf8, false),
    ]))
}
