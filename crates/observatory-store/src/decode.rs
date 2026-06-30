use arrow::array::{Array, ArrayRef, ListArray, RecordBatch, StringArray, StructArray};

use observatory_core::ir::{Atom, ContentNode, LanguageTag};

use crate::error::{Result, StoreError};
use crate::schema::{
    CONTENT_NODES, LANGUAGE_COLUMN, NODE_DATA_FIELD, NODE_KIND_FIELD, NODE_KIND_PLACEHOLDER,
    NODE_KIND_TEXT,
};

pub(crate) fn decode_atoms(batch: &RecordBatch) -> Result<Vec<Atom>> {
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

fn column<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    let array = batch
        .column_by_name(name)
        .ok_or_else(|| StoreError::SchemaMismatch(format!("missing column {name:?}")))?;
    as_array(array, name)
}

fn struct_field<'a, T: Array + 'static>(structs: &'a StructArray, name: &str) -> Result<&'a T> {
    let array = structs
        .column_by_name(name)
        .ok_or_else(|| StoreError::SchemaMismatch(format!("missing content field {name:?}")))?;
    as_array(array, name)
}

fn as_array<'a, T: Array + 'static>(array: &'a ArrayRef, what: &str) -> Result<&'a T> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| StoreError::SchemaMismatch(format!("{what} has an unexpected type")))
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
