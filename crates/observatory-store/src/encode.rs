//! Encoding atoms into an Arrow `RecordBatch` shaped like the atoms `schema`.
//!
//! Pure and infallible: it is handed already-valid `Atom`s and computes each
//! `AtomId` itself, so nothing here can fail — the `expect`s document invariants
//! that hold by construction (a digest is always 32 bytes; the field builders are
//! the types we just built). Batch-first: one call produces one `RecordBatch`
//! (one Lance commit, later), never a per-item write — see `DESIGN.md`.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, FixedSizeBinaryBuilder, ListBuilder, RecordBatch, StringBuilder, StructBuilder,
};
use arrow::datatypes::{DataType, Field};

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode};

use crate::schema::{
    ATOM_ID_WIDTH, NODE, NODE_KIND_PLACEHOLDER, NODE_KIND_TEXT, atoms_schema, content_node_fields,
};

/// Encodes `atoms` into a single `RecordBatch` matching the atoms schema. Each
/// atom's `AtomId` is derived here, never taken from the caller.
pub fn encode_atoms(atoms: &[Atom]) -> RecordBatch {
    let mut ids = FixedSizeBinaryBuilder::new(ATOM_ID_WIDTH);
    let mut languages = StringBuilder::new();

    let node_field = Arc::new(Field::new(
        NODE,
        DataType::Struct(content_node_fields()),
        false,
    ));
    let struct_builder = StructBuilder::from_fields(content_node_fields(), 0);
    let mut contents = ListBuilder::new(struct_builder).with_field(node_field);

    for atom in atoms {
        encode_atom(atom, &mut ids, &mut languages, &mut contents);
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(ids.finish()),
        Arc::new(languages.finish()),
        Arc::new(contents.finish()),
    ];

    RecordBatch::try_new(atoms_schema(), columns)
        .expect("encoded arrays match the atoms schema by construction")
}

/// Appends one atom across all three column builders in lockstep — its id, its
/// language, and its content nodes — so a partially-written atom is impossible.
fn encode_atom(
    atom: &Atom,
    ids: &mut FixedSizeBinaryBuilder,
    languages: &mut StringBuilder,
    contents: &mut ListBuilder<StructBuilder>,
) {
    let id = id_from_atom(atom);
    ids.append_value(id.digest())
        .expect("an AtomId digest is exactly ATOM_ID_WIDTH bytes");
    languages.append_value(atom.language().as_str());

    let nodes = contents.values();
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
    contents.append(true);
}
