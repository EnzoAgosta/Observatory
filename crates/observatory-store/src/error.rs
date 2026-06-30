//! The store's error type. `StoreError` grows one variant per real failure mode;
//! today those are the three the decoder can hit at the trust boundary.

use std::fmt;

use observatory_core::ir::LanguageTagError;

/// Why a store operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// A record batch's columns don't match the atoms schema — a column is
    /// missing, or one has an unexpected Arrow type.
    SchemaMismatch(String),
    /// A stored `language` value is not a valid BCP-47 tag.
    InvalidLanguageTag(LanguageTagError),
    /// A content node's `node_kind` was neither `"text"` nor `"placeholder"`.
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

/// A `Result` whose error is [`StoreError`].
pub type Result<T> = std::result::Result<T, StoreError>;
