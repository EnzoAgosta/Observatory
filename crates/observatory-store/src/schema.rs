//! The Arrow schema for the **atoms** table — the single source of truth for its
//! on-disk column layout, and the wire-format vocabulary (column names and the
//! `node_kind` tag values) that `encode` and `decode` share.
//!
//! | column     | type                                        |
//! |------------|---------------------------------------------|
//! | `atom_id`  | `FixedSizeBinary(32)`                       |
//! | `language` | `Utf8`                                      |
//! | `content`  | `List<Struct<node_kind: Utf8, data: Utf8>>` |
//!
//! Every field is non-nullable: the model has no null node or field, and an atom
//! with no content is an *empty* list, never a null one. A content node is a
//! struct tagged by `node_kind` (`"text"` or `"placeholder"`) rather than an Arrow
//! `Union`, because Lance 7.0.0 cannot store a `Union` (see `DESIGN.md`).

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};

/// Byte width of an `AtomId` digest; Arrow's `FixedSizeBinary` takes an `i32`.
pub(crate) const ATOM_ID_WIDTH: i32 = 32;

pub(crate) const ATOM_ID_COLUMN: &str = "atom_id";
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
            DataType::FixedSizeBinary(ATOM_ID_WIDTH),
            false,
        ),
        Field::new(LANGUAGE_COLUMN, DataType::Utf8, false),
        Field::new(CONTENT_NODES, DataType::List(Arc::new(node_field)), false),
    ]))
}
