use std::sync::Arc;

use arrow::array::{
    ArrayRef, FixedSizeBinaryBuilder, ListBuilder, RecordBatch, StringBuilder, StructBuilder,
};
use arrow::datatypes::{DataType, Field};

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode};

use crate::schema::{
    ATOM_ID_WIDTH, CONTENT_ITEM, NODE_KIND_PLACEHOLDER, NODE_KIND_TEXT, atoms_schema,
    content_node_fields,
};

pub(crate) fn encode_atoms(atoms: &[Atom]) -> RecordBatch {
    let mut id_builder = FixedSizeBinaryBuilder::new(ATOM_ID_WIDTH);
    let mut language_builder = StringBuilder::new();

    let item_field = Arc::new(Field::new(
        CONTENT_ITEM,
        DataType::Struct(content_node_fields()),
        false,
    ));
    let struct_builder = StructBuilder::from_fields(content_node_fields(), 0);
    let mut content_builder = ListBuilder::new(struct_builder).with_field(item_field);

    for atom in atoms {
        let id = id_from_atom(atom);
        id_builder
            .append_value(id.digest())
            .expect("an AtomId digest is exactly ATOM_ID_WIDTH bytes");
        language_builder.append_value(atom.language().as_str());

        let node_builder = content_builder.values();
        for node in atom.content() {
            let (kind, data) = match node {
                ContentNode::Text(data) => (NODE_KIND_TEXT, data),
                ContentNode::Placeholder(data) => (NODE_KIND_PLACEHOLDER, data),
            };
            node_builder
                .field_builder::<StringBuilder>(0)
                .expect("content struct field 0 is the node_kind StringBuilder")
                .append_value(kind);
            node_builder
                .field_builder::<StringBuilder>(1)
                .expect("content struct field 1 is the data StringBuilder")
                .append_value(data);
            node_builder.append(true);
        }
        content_builder.append(true);
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(id_builder.finish()),
        Arc::new(language_builder.finish()),
        Arc::new(content_builder.finish()),
    ];

    RecordBatch::try_new(atoms_schema(), columns)
        .expect("encoded arrays match the atoms schema by construction")
}
