//! The store's error type. `StoreError` grows one variant per real failure mode;
//! today those are the five the decoder can hit at the trust boundary, plus the
//! Lance backend.

use std::fmt;

use observatory_core::ir::LanguageTagError;
use observatory_observations::KindError;

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
    /// A stored `kind` value failed `Kind` construction (empty or whitespace).
    InvalidKind(KindError),
    /// A stored `payload` value is not valid JSON.
    InvalidPayload(serde_json::Error),
    /// The underlying Lance dataset — opening, writing, reading, or maintenance —
    /// returned an error.
    Lance(lance::Error),
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
            Self::InvalidKind(error) => write!(f, "stored observation kind is invalid: {error}"),
            Self::InvalidPayload(error) => write!(f, "stored payload is not valid JSON: {error}"),
            Self::Lance(error) => write!(f, "lance dataset error: {error}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<LanguageTagError> for StoreError {
    fn from(error: LanguageTagError) -> Self {
        Self::InvalidLanguageTag(error)
    }
}

impl From<KindError> for StoreError {
    fn from(error: KindError) -> Self {
        Self::InvalidKind(error)
    }
}

impl From<lance::Error> for StoreError {
    fn from(error: lance::Error) -> Self {
        Self::Lance(error)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidPayload(error)
    }
}

/// A `Result` whose error is [`StoreError`].
pub type Result<T> = std::result::Result<T, StoreError>;
