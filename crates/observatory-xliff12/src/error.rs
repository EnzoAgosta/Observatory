//! The error returned when parsing an XLIFF 1.2 content fragment fails.

use std::fmt;

/// Why [`parse_segment`](crate::parse::parse_segment) couldn't parse a fragment.
#[derive(Debug)]
pub enum ParseError {
    /// Encountered an element outside the XLIFF 1.2 inline content model.
    UnknownTag {
        /// The offending element's local name.
        tag: String,
    },
    /// An XML construct with no place in inline content — a comment, processing
    /// instruction, declaration, or doctype.
    UnsupportedConstruct {
        /// A short label for the construct (e.g. `"comment"`).
        construct: String,
    },
    /// An entity reference that isn't a standard XML entity or numeric character
    /// reference was found in text under
    /// [`EntityMode::Logical`](crate::EntityMode::Logical).
    UnknownEntity {
        /// The reference as written, e.g. `&nbsp;`.
        entity: String,
    },
    /// The underlying XML reader failed.
    QuickXmlError {
        /// The underlying error.
        error: quick_xml::Error,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTag { tag } => {
                write!(f, "{tag:?} is not a known XLIFF 1.2 inline element")
            }
            Self::UnknownEntity { entity } => {
                write!(f, "{entity:?} is not a standard XML entity")
            }
            Self::QuickXmlError { error } => write!(f, "quick_xml had an error: {error}"),
            Self::UnsupportedConstruct { construct } => {
                write!(
                    f,
                    "unsupported XML construct in segment content: {construct}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<quick_xml::Error> for ParseError {
    fn from(error: quick_xml::Error) -> Self {
        Self::QuickXmlError { error }
    }
}
