//! The Arrow schema for the **atoms** table — the single source of truth for its
//! on-disk column layout, plus the wire-format vocabulary (column names and the
//! `node_kind` tag values) that encoding and decoding share.
//!
//! The schema is:
//!
//! | column     | type                                        |
//! |------------|---------------------------------------------|
//! | `atom_id`  | `FixedSizeBinary(32)`                       |
//! | `language` | `Utf8`                                      |
//! | `content`  | `List<Struct<node_kind: Utf8, data: Utf8>>` |
//!
//! Every field is non-nullable: the model has no null node or null field, and an
//! atom with no content is an *empty* list, never a null one. The `content`
//! element is a struct tagged by `node_kind` (`"text"` or `"placeholder"`)
//! carrying the node's raw bytes in `data` — Lance 7.0.0 cannot store an Arrow
//! `Union`, so the variant is a named tag rather than a true sum type (see
//! `DESIGN.md`).

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};

/// The 32-byte width of an `AtomId` digest, as Arrow's `FixedSizeBinary` takes
/// its length as an `i32`.
pub(crate) const ATOM_ID_WIDTH: i32 = 32;

/// Name of the content-addressed key column.
pub(crate) const ATOM_ID_COLUMN: &str = "atom_id";
/// Name of the BCP-47 language-tag column.
pub(crate) const LANGUAGE_COLUMN: &str = "language";
/// Name of the content-nodes column.
pub(crate) const CONTENT_COLUMN: &str = "content";

/// Name Arrow gives a list's element field, by convention.
pub(crate) const CONTENT_ITEM: &str = "item";
/// Name of the per-node variant-tag field inside the content struct.
pub(crate) const NODE_KIND_FIELD: &str = "node_kind";
/// Name of the per-node raw-data field inside the content struct.
pub(crate) const NODE_DATA_FIELD: &str = "data";

/// The `node_kind` tag for a text content node (`ContentNode::Text`).
pub(crate) const NODE_KIND_TEXT: &str = "text";
/// The `node_kind` tag for a placeholder content node (`ContentNode::Placeholder`).
pub(crate) const NODE_KIND_PLACEHOLDER: &str = "placeholder";

/// The struct type of one content node: `Struct<node_kind: Utf8, data: Utf8>`.
fn content_node_type() -> DataType {
    DataType::Struct(Fields::from(vec![
        Field::new(NODE_KIND_FIELD, DataType::Utf8, false),
        Field::new(NODE_DATA_FIELD, DataType::Utf8, false),
    ]))
}

/// The Arrow schema of the atoms table.
pub(crate) fn atoms_schema() -> SchemaRef {
    let content_element = Field::new(CONTENT_ITEM, content_node_type(), false);
    Arc::new(Schema::new(vec![
        Field::new(
            ATOM_ID_COLUMN,
            DataType::FixedSizeBinary(ATOM_ID_WIDTH),
            false,
        ),
        Field::new(LANGUAGE_COLUMN, DataType::Utf8, false),
        Field::new(
            CONTENT_COLUMN,
            DataType::List(Arc::new(content_element)),
            false,
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_three_columns_in_order() {
        let schema = atoms_schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(names, [ATOM_ID_COLUMN, LANGUAGE_COLUMN, CONTENT_COLUMN]);
    }

    #[test]
    fn every_top_level_field_is_non_nullable() {
        let schema = atoms_schema();
        assert!(schema.fields().iter().all(|f| !f.is_nullable()));
    }

    #[test]
    fn atom_id_is_fixed_size_binary_of_the_digest_width() {
        let schema = atoms_schema();
        let field = schema.field_with_name(ATOM_ID_COLUMN).unwrap();
        assert_eq!(field.data_type(), &DataType::FixedSizeBinary(ATOM_ID_WIDTH));
    }

    #[test]
    fn language_is_utf8() {
        let schema = atoms_schema();
        let field = schema.field_with_name(LANGUAGE_COLUMN).unwrap();
        assert_eq!(field.data_type(), &DataType::Utf8);
    }

    #[test]
    fn content_is_a_non_null_list_of_the_node_struct() {
        let schema = atoms_schema();
        let field = schema.field_with_name(CONTENT_COLUMN).unwrap();
        let DataType::List(element) = field.data_type() else {
            panic!("content must be a List, got {:?}", field.data_type());
        };
        assert_eq!(element.name(), CONTENT_ITEM);
        assert!(!element.is_nullable());
        assert_eq!(element.data_type(), &content_node_type());
    }

    #[test]
    fn the_node_struct_has_two_non_null_utf8_fields() {
        let DataType::Struct(fields) = content_node_type() else {
            panic!("a content node must be a Struct");
        };
        let described: Vec<(&str, &DataType, bool)> = fields
            .iter()
            .map(|f| (f.name().as_str(), f.data_type(), f.is_nullable()))
            .collect();
        assert_eq!(
            described,
            [
                (NODE_KIND_FIELD, &DataType::Utf8, false),
                (NODE_DATA_FIELD, &DataType::Utf8, false),
            ]
        );
    }
}
