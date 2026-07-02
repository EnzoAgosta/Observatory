//! The store error type — backend-agnostic.
//!
//! [`StoreError`] carries the variants every backend shares (a corrupt row at
//! the read-back trust boundary, an invalid domain type encountered in stored
//! bytes) plus a single [`Backend`](StoreError::Backend) escape hatch for a
//! concrete implementation's own failures (open, write, maintenance). The
//! backend maps its native errors into `Backend` at the trait boundary; the
//! trait never names a backend's error type.

use std::fmt;

use observatory_core::ir::LanguageTagError;
use observatory_observations::KindError;

/// Why a store operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// A record batch's columns don't match the atoms or observations schema —
    /// a column is missing, or one has an unexpected Arrow type. Surfaced by a
    /// backend's decoder at the trust boundary.
    SchemaMismatch(String),
    /// A stored `language` value is not a valid BCP-47 tag.
    InvalidLanguageTag(LanguageTagError),
    /// A content node's `node_kind` was neither `"text"` nor `"placeholder"`.
    UnknownNodeKind(String),
    /// A stored `kind` value failed `Kind` construction (empty or whitespace).
    InvalidKind(KindError),
    /// A stored `payload` value is not valid JSON.
    InvalidPayload(serde_json::Error),
    /// A backend-specific failure (open, write, index, maintenance, …) that
    /// does not have a domain-level reading. The concrete backend's error is
    /// stringified at the trait boundary; the chain is preserved via
    /// [`Error::source`](std::error::Error::source) when the backend hands
    /// over a boxed source.
    Backend {
        /// Human-readable summary of the backend failure.
        detail: String,
        /// The original backend error, if the backend chose to preserve it
        /// for chain inspection. `None` when the backend had no structured
        /// source to forward (e.g. it constructed the failure itself).
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaMismatch(detail) => {
                write!(f, "record batch does not match the schema: {detail}")
            }
            Self::InvalidLanguageTag(error) => {
                write!(f, "stored language tag is invalid: {error}")
            }
            Self::UnknownNodeKind(kind) => write!(f, "unknown content node_kind: {kind:?}"),
            Self::InvalidKind(error) => write!(f, "stored observation kind is invalid: {error}"),
            Self::InvalidPayload(error) => write!(f, "stored payload is not valid JSON: {error}"),
            Self::Backend { detail, source: _ } => {
                write!(f, "store backend error: {detail}")
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLanguageTag(error) => Some(error),
            Self::InvalidKind(error) => Some(error),
            Self::InvalidPayload(error) => Some(error),
            Self::Backend { source, .. } => source.as_deref().map(|source| source as &(dyn std::error::Error + 'static)),
            Self::SchemaMismatch(_)
            | Self::UnknownNodeKind(_) => None,
        }
    }
}

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

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidPayload(error)
    }
}

/// A `Result` whose error is [`StoreError`].
pub type Result<T> = std::result::Result<T, StoreError>;
