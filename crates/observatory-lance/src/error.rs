//! Lance's local `Result` alias and a single bridge from `lance::Error` into
//! the backend-agnostic [`StoreError`].
//!
//! The orphan rule forbids `impl From<lance::Error> for StoreError` in this
//! crate (both types are foreign), so Lance error sites call
//! [`map_lance`](self::map_lance) explicitly rather than relying on `?`'s
//! auto-conversion. Domain-level errors (`LanguageTagError`, `KindError`,
//! `serde_json::Error`) already have `From` impls on `StoreError` in the
//! trait crate, so `?` keeps working for them.

use observatory_store::StoreError;

/// A `Result` whose error is [`StoreError`] — the same type the trait crate
/// uses, re-aliased here so Lance-internal call sites read naturally.
pub(crate) type Result<T> = std::result::Result<T, StoreError>;

/// Maps a `lance::Error` into a `StoreError::Backend`, preserving the
/// original as the error-chain source for `Error::source()` inspection.
pub(crate) fn map_lance(error: lance::Error) -> StoreError {
    StoreError::Backend {
        detail: error.to_string(),
        source: Some(Box::new(error)),
    }
}
