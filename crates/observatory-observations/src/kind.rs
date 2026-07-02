//! What an observation asserts: its [`Kind`].

use std::fmt;

/// The label naming what an observation asserts — `"translation_of"`,
/// `"blacklisted"`, `"approved_by"`, and so on.
///
/// A `Kind` is an open, user-definable label. It is validated only for
/// *structure* — it must not be empty — never for *meaning*: which kinds exist,
/// which arity each expects, and what payload each carries are semantic policy
/// that lives above this crate. This mirrors
/// [`LanguageTag`](observatory_core::ir::LanguageTag), which checks a tag is
/// well-formed without consulting a registry of "real" locales.
///
/// The label is stored verbatim — case and whitespace are significant and are
/// never folded — so keeping labels consistent (a single spelling of
/// `"translation_of"`) is the caller's job, the same way the caller chooses a
/// normalization before taking an atom's id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Kind(String);

impl Kind {
    /// Builds a `Kind` from a label.
    ///
    /// # Errors
    /// [`KindError::Empty`] if `label` is empty or contains only whitespace.
    pub fn new(label: impl Into<String>) -> Result<Self, KindError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(KindError::Empty);
        }
        Ok(Self(label))
    }

    /// The label as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a [`Kind`] failed to construct.
#[derive(Debug)]
pub enum KindError {
    /// The label was empty or contained only whitespace.
    Empty,
}

impl fmt::Display for KindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "an observation kind must not be empty"),
        }
    }
}

impl std::error::Error for KindError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_label_and_stores_it_verbatim() {
        let kind = Kind::new("translation_of").unwrap();
        assert_eq!(kind.as_str(), "translation_of");
    }

    #[test]
    fn preserves_case_and_whitespace_in_the_label() {
        let kind = Kind::new(" Translation_Of ").unwrap();
        assert_eq!(kind.as_str(), " Translation_Of ");
    }

    #[test]
    fn rejects_an_empty_label() {
        assert!(matches!(Kind::new(""), Err(KindError::Empty)));
    }

    #[test]
    fn rejects_a_whitespace_only_label() {
        assert!(matches!(Kind::new("   "), Err(KindError::Empty)));
    }

    #[test]
    fn empty_error_displays_a_reason() {
        assert!(KindError::Empty.to_string().contains("must not be empty"));
    }

    #[test]
    fn display_emits_the_label_verbatim() {
        let kind = Kind::new("translation_of").unwrap();
        assert_eq!(kind.to_string(), "translation_of");
        assert_eq!(
            Kind::new(" approved_by ").unwrap().to_string(),
            " approved_by "
        );
    }
}
