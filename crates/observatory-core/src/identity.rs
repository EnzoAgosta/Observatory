use sha2::{Digest, Sha256};

use crate::ir::{Atom, ContentNode};

const SERIALIZATION_VERSION: u8 = 0;

const TAG_TEXT: u8 = 0x00;

const TAG_PLACEHOLDER: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomId([u8; 32]);

impl AtomId {
    pub fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn digest(&self) -> [u8; 32] {
        self.0
    }
}

fn content_as_bytes(content: &[ContentNode]) -> Vec<u8> {
    let mut buf = Vec::new();
    for node in content {
        match node {
            ContentNode::Placeholder(_) => {
                buf.push(TAG_PLACEHOLDER);
            }
            ContentNode::Text(data) => {
                buf.push(TAG_TEXT);
                add_bytes_with_be_length(&mut buf, data.as_bytes());
            }
        }
    }
    buf
}

fn add_bytes_with_be_length(buf: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("field exceeds u32::MAX bytes");
    buf.extend(len.to_be_bytes());
    buf.extend(bytes);
}

fn language_as_bytes(language: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    add_bytes_with_be_length(&mut buf, language.as_bytes());
    buf
}

fn canonical_bytes(language: &str, content: &[ContentNode]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(SERIALIZATION_VERSION);
    buf.extend(language_as_bytes(language));
    buf.extend(content_as_bytes(content));
    buf
}

pub fn id_from_atom(atom: &Atom) -> AtomId {
    let bytes = canonical_bytes(atom.language().as_str(), atom.content());
    let digest = Sha256::digest(bytes);
    AtomId::from_digest(digest.into())
}
