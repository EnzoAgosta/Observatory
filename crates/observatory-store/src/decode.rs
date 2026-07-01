//! Decoding atoms and observations `RecordBatch`es back into domain types.
//!
//! This is the trust boundary: it reads data that may be malformed (a foreign or
//! corrupt dataset), so unlike `encode` it is fallible and returns `StoreError`.
//! Concrete arrays are recovered from the type-erased columns with
//! `as_any().downcast_ref()` — a checked, runtime cast — and a missing or
//! wrong-typed column, an unknown `node_kind`, an invalid language tag, an empty
//! kind, or a malformed JSON payload each become a `StoreError` rather than a
//! panic.

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryArray, ListArray, RecordBatch, StringArray, StructArray,
    TimestampMicrosecondArray,
};
use observatory_core::identity::AtomId;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_observations::identity::micros_to_system_time;
use observatory_observations::{Kind, Observation};

use crate::error::{Result, StoreError};
use crate::schema::{
    CONTENT_NODES, EFFECTIVE_AT_COLUMN, KIND_COLUMN, LANGUAGE_COLUMN, NODE_DATA_FIELD,
    NODE_KIND_FIELD, NODE_KIND_PLACEHOLDER, NODE_KIND_TEXT, PAYLOAD_COLUMN, RECORDED_AT_COLUMN,
    SUBJECTS_COLUMN, DIGEST_WIDTH,
};

/// Reconstructs every atom in `batch` by reading `column[row]` across the
/// columns. Errors if a column is missing or mistyped, a stored language tag is
/// invalid, or a node carries an unknown `node_kind`.
pub fn decode_atoms(batch: &RecordBatch) -> Result<Vec<Atom>> {
    let languages = column::<StringArray>(batch, LANGUAGE_COLUMN)?;
    let contents = column::<ListArray>(batch, CONTENT_NODES)?;

    let mut atoms = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let language = LanguageTag::from_string(languages.value(row))?;
        let nodes = decode_content(&contents.value(row))?;
        atoms.push(Atom::new(language, nodes));
    }
    Ok(atoms)
}

/// Decodes one list element — the struct rows for a single atom — into its
/// content nodes.
fn decode_content(nodes: &ArrayRef) -> Result<Vec<ContentNode>> {
    let structs = as_array::<StructArray>(nodes, "content element")?;
    let kinds = struct_field::<StringArray>(structs, NODE_KIND_FIELD)?;
    let datas = struct_field::<StringArray>(structs, NODE_DATA_FIELD)?;

    let mut decoded = Vec::with_capacity(structs.len());
    for i in 0..structs.len() {
        let data = datas.value(i);
        let node = match kinds.value(i) {
            NODE_KIND_TEXT => ContentNode::text(data),
            NODE_KIND_PLACEHOLDER => ContentNode::placeholder(data),
            other => return Err(StoreError::UnknownNodeKind(other.to_owned())),
        };
        decoded.push(node);
    }
    Ok(decoded)
}

/// Fetches column `name` from `batch` and downcasts it to `T`, or `SchemaMismatch`.
fn column<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    let array = batch
        .column_by_name(name)
        .ok_or_else(|| StoreError::SchemaMismatch(format!("missing column {name:?}")))?;
    as_array(array, name)
}

/// Like `column`, but fetches field `name` from a struct array's children.
fn struct_field<'a, T: Array + 'static>(structs: &'a StructArray, name: &str) -> Result<&'a T> {
    let array = structs
        .column_by_name(name)
        .ok_or_else(|| StoreError::SchemaMismatch(format!("missing content field {name:?}")))?;
    as_array(array, name)
}

/// Runtime downcast of a type-erased array to the concrete `T`, or `SchemaMismatch`.
fn as_array<'a, T: Array + 'static>(array: &'a ArrayRef, what: &str) -> Result<&'a T> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| StoreError::SchemaMismatch(format!("{what} has an unexpected type")))
}

/// Reconstructs every observation in `batch` by reading `column[row]` across the
/// columns. Errors if a column is missing or mistyped, a stored `kind` is empty,
/// or a stored `payload` is not valid JSON.
pub fn decode_observations(batch: &RecordBatch) -> Result<Vec<Observation>> {
    let kinds = column::<StringArray>(batch, KIND_COLUMN)?;
    let subjects = column::<ListArray>(batch, SUBJECTS_COLUMN)?;
    let recorded_at = column::<TimestampMicrosecondArray>(batch, RECORDED_AT_COLUMN)?;
    let effective_at = column::<TimestampMicrosecondArray>(batch, EFFECTIVE_AT_COLUMN)?;
    let payloads = column::<StringArray>(batch, PAYLOAD_COLUMN)?;

    let mut observations = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let kind = Kind::new(kinds.value(row))?;
        let subjects = decode_subjects(&subjects.value(row))?;
        let recorded_at = micros_to_system_time(recorded_at.value(row));
        let effective_at = micros_to_system_time(effective_at.value(row));
        let payload = serde_json::from_str(payloads.value(row))?;
        observations.push(Observation::new(
            kind,
            subjects,
            recorded_at,
            effective_at,
            payload,
        ));
    }
    Ok(observations)
}

/// Decodes one subjects list — the `FixedSizeBinary(32)` elements for a single
/// observation — into its `AtomId`s. Errors if the element array is mistyped or
/// a digest is not exactly 32 bytes.
fn decode_subjects(list: &ArrayRef) -> Result<Vec<AtomId>> {
    let digests = as_array::<FixedSizeBinaryArray>(list, "subjects element")?;
    let mut subjects = Vec::with_capacity(digests.len());
    for i in 0..digests.len() {
        let bytes: &[u8] = digests.value(i);
        let digest: [u8; DIGEST_WIDTH as usize] = bytes.try_into().map_err(|_| {
            StoreError::SchemaMismatch(format!(
                "a subject digest is {} bytes, expected {DIGEST_WIDTH}",
                bytes.len()
            ))
        })?;
        subjects.push(AtomId::from_digest(digest));
    }
    Ok(subjects)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{FixedSizeBinaryBuilder, ListBuilder, StringBuilder, StructBuilder};
    use arrow::datatypes::{DataType, Field, Schema};

    use super::*;
    use crate::encode::encode_atoms;
    use crate::schema::{NODE, atoms_schema, content_node_fields};

    fn atom(language: &str, nodes: Vec<ContentNode>) -> Atom {
        Atom::new(LanguageTag::from_string(language).unwrap(), nodes)
    }

    /// Builds an atoms-schema batch from raw `(node_kind, data)` tuples, bypassing
    /// `encode` so tests can plant values `encode` would never produce — an
    /// unknown `node_kind`, an invalid language tag.
    fn raw_batch(rows: &[(&str, &[(&str, &str)])]) -> RecordBatch {
        let mut ids = FixedSizeBinaryBuilder::new(32);
        let mut digests = FixedSizeBinaryBuilder::new(32);
        let mut languages = StringBuilder::new();
        let node_field = Arc::new(Field::new(
            NODE,
            DataType::Struct(content_node_fields()),
            false,
        ));
        let mut contents = ListBuilder::new(StructBuilder::from_fields(content_node_fields(), 0))
            .with_field(node_field);

        for (language, nodes) in rows {
            ids.append_value([0u8; 32]).unwrap();
            digests.append_value([0u8; 32]).unwrap();
            languages.append_value(language);
            let node_builder = contents.values();
            for (kind, data) in *nodes {
                node_builder
                    .field_builder::<StringBuilder>(0)
                    .unwrap()
                    .append_value(kind);
                node_builder
                    .field_builder::<StringBuilder>(1)
                    .unwrap()
                    .append_value(data);
                node_builder.append(true);
            }
            contents.append(true);
        }

        RecordBatch::try_new(
            atoms_schema(),
            vec![
                Arc::new(ids.finish()),
                Arc::new(digests.finish()),
                Arc::new(languages.finish()),
                Arc::new(contents.finish()),
            ],
        )
        .unwrap()
    }

    #[test]
    fn round_trips_a_variety_of_atoms() {
        let atoms = vec![
            atom("en-US", vec![]),
            atom("en-US", vec![ContentNode::text("hello")]),
            atom("fr-FR", vec![ContentNode::placeholder("<x/>")]),
            atom(
                "de-DE",
                vec![
                    ContentNode::text("Click "),
                    ContentNode::placeholder("<b>"),
                    ContentNode::text("here"),
                    ContentNode::placeholder("</b>"),
                ],
            ),
            atom("ja-JP", vec![ContentNode::text("こんにちは")]),
        ];
        let batch = encode_atoms(&atoms);
        assert_eq!(decode_atoms(&batch).unwrap(), atoms);
    }

    #[test]
    fn round_trips_an_empty_batch() {
        let batch = encode_atoms(&[]);
        assert!(decode_atoms(&batch).unwrap().is_empty());
    }

    #[test]
    fn rejects_unknown_node_kind() {
        let batch = raw_batch(&[("en-US", &[("bogus", "x")])]);
        let error = decode_atoms(&batch).unwrap_err();
        assert!(matches!(error, StoreError::UnknownNodeKind(kind) if kind == "bogus"));
    }

    #[test]
    fn rejects_invalid_language_tag() {
        let batch = raw_batch(&[("en", &[])]); // well-formed but missing a region subtag
        let error = decode_atoms(&batch).unwrap_err();
        assert!(matches!(error, StoreError::InvalidLanguageTag(_)));
    }

    #[test]
    fn rejects_missing_column() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            LANGUAGE_COLUMN,
            DataType::Utf8,
            false,
        )]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["en-US"]))]).unwrap();
        let error = decode_atoms(&batch).unwrap_err();
        assert!(matches!(error, StoreError::SchemaMismatch(_)));
    }

    #[test]
    fn rejects_wrong_column_type() {
        let schema = Arc::new(Schema::new(vec![
            Field::new(LANGUAGE_COLUMN, DataType::Utf8, false),
            Field::new(CONTENT_NODES, DataType::Utf8, false), // should be a List, not Utf8
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["en-US"])),
                Arc::new(StringArray::from(vec!["oops"])),
            ],
        )
        .unwrap();
        let error = decode_atoms(&batch).unwrap_err();
        assert!(matches!(error, StoreError::SchemaMismatch(_)));
    }
}

#[cfg(test)]
mod observation_tests {
    use std::sync::Arc;

    use arrow::array::{
        FixedSizeBinaryBuilder, ListBuilder, RecordBatch, StringBuilder, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema};

    use super::*;
    use crate::encode::encode_observations;
    use crate::schema::{SUBJECT_FIELD, observations_schema};

    use observatory_core::identity::AtomId;
    use observatory_observations::Kind;
    use observatory_observations::Observation;
    use serde_json::{Value, json};
    use std::time::{Duration, SystemTime};

    fn atom_id(lang: &str, text: &str) -> AtomId {
        use observatory_core::identity::id_from_atom;
        use observatory_core::ir::{Atom, ContentNode, LanguageTag};
        id_from_atom(&Atom::new(
            LanguageTag::from_string(lang).unwrap(),
            [ContentNode::text(text)],
        ))
    }

    fn kind(label: &str) -> Kind {
        Kind::new(label).unwrap()
    }

    #[test]
    fn round_trips_a_variety_of_observations() {
        let observations = vec![
            Observation::property(
                kind("blacklisted"),
                atom_id("en-US", "Hello"),
                SystemTime::UNIX_EPOCH,
                None,
                json!({ "reason": "offensive" }),
            ),
            Observation::relationship(
                kind("translation_of"),
                vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
                SystemTime::UNIX_EPOCH,
                None,
                Value::Null,
            ),
            Observation::relationship(
                kind("context_for"),
                vec![
                    atom_id("en-US", "Hello"),
                    atom_id("fr-FR", "Bonjour"),
                    atom_id("de-DE", "Hallo"),
                ],
                SystemTime::UNIX_EPOCH + Duration::from_secs(86400),
                Some(SystemTime::UNIX_EPOCH),
                json!({ "author": "deepl:v2", "confidence": 0.91 }),
            ),
        ];
        let batch = encode_observations(&observations);
        assert_eq!(decode_observations(&batch).unwrap(), observations);
    }

    #[test]
    fn round_trips_an_empty_batch() {
        let batch = encode_observations(&[]);
        assert!(decode_observations(&batch).unwrap().is_empty());
    }

    #[test]
    fn round_trips_pre_epoch_effective_at() {
        let observations = vec![Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH,
            Some(SystemTime::UNIX_EPOCH - Duration::from_secs(60 * 60 * 24 * 365)),
            json!({ "reviewer": "alice" }),
        )];
        let batch = encode_observations(&observations);
        assert_eq!(decode_observations(&batch).unwrap(), observations);
    }

    #[test]
    fn round_trips_pre_epoch_recorded_at() {
        let observations = vec![Observation::property(
            kind("approved_by"),
            atom_id("en-US", "Hello"),
            SystemTime::UNIX_EPOCH - Duration::from_secs(60 * 60 * 24 * 365),
            None,
            json!({ "reviewer": "alice" }),
        )];
        let batch = encode_observations(&observations);
        assert_eq!(decode_observations(&batch).unwrap(), observations);
    }

    fn raw_observation_batch(rows: &[(&str, Vec<[u8; 32]>, i64, i64, &str)]) -> RecordBatch {
        let mut observation_ids = FixedSizeBinaryBuilder::new(DIGEST_WIDTH);
        let mut kinds = StringBuilder::new();
        let subject_field = Arc::new(Field::new(
            SUBJECT_FIELD,
            DataType::FixedSizeBinary(DIGEST_WIDTH),
            false,
        ));
        let mut subjects =
            ListBuilder::new(FixedSizeBinaryBuilder::new(DIGEST_WIDTH)).with_field(subject_field);
        let mut recorded_at_values: Vec<i64> = Vec::new();
        let mut effective_at_values: Vec<i64> = Vec::new();
        let mut payloads = StringBuilder::new();

        for (kind, subject_digests, recorded_micros, effective_micros, payload) in rows {
            observation_ids.append_value([0u8; 32]).unwrap();
            kinds.append_value(kind);
            let subject_builder = subjects.values();
            for digest in subject_digests {
                subject_builder.append_value(*digest).unwrap();
            }
            subjects.append(true);
            recorded_at_values.push(*recorded_micros);
            effective_at_values.push(*effective_micros);
            payloads.append_value(payload);
        }

        let recorded_at =
            TimestampMicrosecondArray::from(recorded_at_values).with_timezone("UTC");
        let effective_at =
            TimestampMicrosecondArray::from(effective_at_values).with_timezone("UTC");

        RecordBatch::try_new(
            observations_schema(),
            vec![
                Arc::new(observation_ids.finish()),
                Arc::new(kinds.finish()),
                Arc::new(subjects.finish()),
                Arc::new(recorded_at),
                Arc::new(effective_at),
                Arc::new(payloads.finish()),
            ],
        )
        .unwrap()
    }

    #[test]
    fn rejects_an_empty_kind() {
        let batch = raw_observation_batch(&[("", vec![[0u8; 32]], 0, 0, "null")]);
        let error = decode_observations(&batch).unwrap_err();
        assert!(matches!(error, StoreError::InvalidKind(_)));
    }

    #[test]
    fn rejects_a_whitespace_kind() {
        let batch = raw_observation_batch(&[("   ", vec![[0u8; 32]], 0, 0, "null")]);
        let error = decode_observations(&batch).unwrap_err();
        assert!(matches!(error, StoreError::InvalidKind(_)));
    }

    #[test]
    fn rejects_malformed_payload() {
        let batch = raw_observation_batch(&[("approved_by", vec![[0u8; 32]], 0, 0, "not json")]);
        let error = decode_observations(&batch).unwrap_err();
        assert!(matches!(error, StoreError::InvalidPayload(_)));
    }

    #[test]
    fn rejects_missing_column() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            KIND_COLUMN,
            DataType::Utf8,
            false,
        )]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["approved_by"]))])
                .unwrap();
        let error = decode_observations(&batch).unwrap_err();
        assert!(matches!(error, StoreError::SchemaMismatch(_)));
    }

    #[test]
    fn rejects_wrong_column_type() {
        let schema = Arc::new(Schema::new(vec![
            Field::new(KIND_COLUMN, DataType::Utf8, false),
            // Should be List<FixedSizeBinary(32)>, not Utf8
            Field::new(SUBJECTS_COLUMN, DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["approved_by"])),
                Arc::new(StringArray::from(vec!["oops"])),
            ],
        )
        .unwrap();
        let error = decode_observations(&batch).unwrap_err();
        assert!(matches!(error, StoreError::SchemaMismatch(_)));
    }
}
