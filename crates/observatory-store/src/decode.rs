use arrow::array::{Array, ArrayRef, ListArray, RecordBatch, StringArray, StructArray};

use observatory_core::ir::{Atom, ContentNode, LanguageTag};

use crate::error::{Result, StoreError};
use crate::schema::{
    CONTENT_COLUMN, LANGUAGE_COLUMN, NODE_DATA_FIELD, NODE_KIND_FIELD, NODE_KIND_PLACEHOLDER,
    NODE_KIND_TEXT,
};

pub(crate) fn decode_atoms(batch: &RecordBatch) -> Result<Vec<Atom>> {
    let languages = column::<StringArray>(batch, LANGUAGE_COLUMN)?;
    let contents = column::<ListArray>(batch, CONTENT_COLUMN)?;

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
