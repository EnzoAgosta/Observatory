//! Why [`parse`](crate::parse) failed.

use std::fmt;

/// An error from parsing an XLIFF 1.2 content fragment into an [`Atom`].
///
/// The adapter assumes it is handed already-extracted, well-formed XLIFF content
/// (D26); these variants report the ways that assumption can break. The
/// underlying XML library never appears in this type — it stays swappable behind
/// the boundary.
///
/// [`Atom`]: observatory_core::ir::Atom
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The fragment is not well-formed XML, or violates the XLIFF inline
    /// structure (e.g. a mismatched end tag). `detail` carries the underlying
    /// description.
    Malformed {
        /// A human-readable description of what was malformed.
        detail: String,
    },
    /// A named entity outside the standard XML set (`amp`, `lt`, `gt`, `apos`,
    /// `quot`) was found in text under [`EntityMode::Logical`](crate::EntityMode::Logical).
    /// We refuse to guess a replacement; resolve it a layer up or use
    /// [`EntityMode::Verbatim`](crate::EntityMode::Verbatim).
    UnknownEntity {
        /// The entity name as written, without the surrounding `&` and `;`.
        entity: String,
    },
    /// An element that is not part of the XLIFF 1.2 inline content model
    /// (`g`, `x`, `bx`, `ex`, `bpt`, `ept`, `ph`, `it`, `mrk`) appeared in the
    /// fragment.
    UnexpectedElement {
        /// The offending element's local name.
        name: String,
    },
    /// An XML construct that has no place in an inline content fragment — a
    /// comment, processing instruction, declaration, or doctype — appeared.
    UnsupportedConstruct {
        /// A short label for the construct (e.g. `"comment"`).
        construct: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { detail } => write!(f, "malformed XLIFF content: {detail}"),
            Self::UnknownEntity { entity } => {
                write!(f, "unknown XML entity in text: &{entity};")
            }
            Self::UnexpectedElement { name } => {
                write!(f, "element {name:?} is not an XLIFF 1.2 inline element")
            }
            Self::UnsupportedConstruct { construct } => {
                write!(f, "unsupported XML construct in content: {construct}")
            }
        }
    }
}

impl std::error::Error for ParseError {}
