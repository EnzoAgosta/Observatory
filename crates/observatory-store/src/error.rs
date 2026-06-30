use std::fmt;

use observatory_core::ir::LanguageTagError;

#[derive(Debug)]
pub(crate) enum StoreError {
    SchemaMismatch(String),
    InvalidLanguageTag(LanguageTagError),
    UnknownNodeKind(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaMismatch(detail) => {
                write!(f, "record batch does not match the atoms schema: {detail}")
            }
            Self::InvalidLanguageTag(error) => {
                write!(f, "stored language tag is invalid: {error}")
            }
            Self::UnknownNodeKind(kind) => write!(f, "unknown content node_kind: {kind:?}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<LanguageTagError> for StoreError {
    fn from(error: LanguageTagError) -> Self {
        Self::InvalidLanguageTag(error)
    }
}

pub(crate) type Result<T> = std::result::Result<T, StoreError>;
