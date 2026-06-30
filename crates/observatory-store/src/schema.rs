use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};

pub(crate) const ATOM_ID_WIDTH: i32 = 32;

pub(crate) const ATOM_ID_COLUMN: &str = "atom_id";
pub(crate) const LANGUAGE_COLUMN: &str = "language";
pub(crate) const CONTENT_COLUMN: &str = "content";

pub(crate) const CONTENT_ITEM: &str = "item";
pub(crate) const NODE_KIND_FIELD: &str = "node_kind";
pub(crate) const NODE_DATA_FIELD: &str = "data";

pub(crate) const NODE_KIND_TEXT: &str = "text";
pub(crate) const NODE_KIND_PLACEHOLDER: &str = "placeholder";

pub(crate) fn content_node_fields() -> Fields {
    Fields::from(vec![
        Field::new(NODE_KIND_FIELD, DataType::Utf8, false),
        Field::new(NODE_DATA_FIELD, DataType::Utf8, false),
    ])
}

pub(crate) fn atoms_schema() -> SchemaRef {
    let content_element = Field::new(CONTENT_ITEM, DataType::Struct(content_node_fields()), false);
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
