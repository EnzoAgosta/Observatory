//! The data model: an [`Atom`] and the pieces it is built from.
//!
//! An [`Atom`] is an ordered sequence of [`ContentNode`]s in a single
//! [`LanguageTag`]. Each node is either translatable text or an opaque
//! *placeholder* standing in for non-text (an inline tag, a variable, a code).
//!
//! Construction is *faithful*: an `Atom` records exactly the nodes it is given —
//! no merging, dropping, or reordering — and the original string is recovered by
//! joining every node's data in order ([`Atom::reconstruct`]). Identity is a
//! separate, derived projection (the `AtomId`; see [`crate::identity`]): two atoms
//! can be structurally different (`!=`) yet share an identity, so dedup and
//! comparison go through the `AtomId`, never `==`.

use std::fmt;

use oxilangtag::{LanguageTag as OxiLanguageTag, LanguageTagParseError};

/// A single-language string recorded as an ordered run of [`ContentNode`]s.
///
/// The order of [`content`](Atom::content) is significant. Construction is
/// faithful — an `Atom` preserves exactly the nodes it was built from, including
/// adjacent and empty text runs — so `==` means "structurally identical," not
/// "same identity." Identity is the `AtomId` (see [`crate::identity`]); two atoms
/// that differ only in how their text was split into runs share an `AtomId`
/// without being `==`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    language: LanguageTag,
    content: Vec<ContentNode>,
}

impl Atom {
    /// Records `nodes` into an `Atom` exactly as given — no merging, dropping, or
    /// reordering.
    pub fn new(language: LanguageTag, nodes: impl IntoIterator<Item = ContentNode>) -> Self {
        Self {
            language,
            content: nodes.into_iter().collect(),
        }
    }

    /// The language of this atom.
    pub fn language(&self) -> &LanguageTag {
        &self.language
    }

    /// The atom's content nodes, in order.
    pub fn content(&self) -> &[ContentNode] {
        &self.content
    }

    /// Reconstructs the original string: the in-order join of every node's data.
    pub fn reconstruct(&self) -> String {
        self.content.iter().map(ContentNode::as_str).collect()
    }
}

/// One run of an [`Atom`]: either translatable text or an opaque placeholder.
///
/// The wrapped `String` is the raw recorded content and is never interpreted. For
/// [`Placeholder`](ContentNode::Placeholder) it is the original markup (an inline
/// tag, a variable, …); for [`Text`](ContentNode::Text) it is the text itself.
/// Code that cares about the distinction matches the variant; code that only
/// needs the bytes uses [`as_str`](ContentNode::as_str).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentNode {
    /// A translatable text run.
    Text(String),
    /// An opaque placeholder; the data is its raw, uninterpreted markup.
    Placeholder(String),
}

impl ContentNode {
    /// Builds a [`Text`](ContentNode::Text) run.
    pub fn text(data: impl Into<String>) -> Self {
        Self::Text(data.into())
    }

    /// Builds a [`Placeholder`](ContentNode::Placeholder) from its raw markup.
    pub fn placeholder(data: impl Into<String>) -> Self {
        Self::Placeholder(data.into())
    }

    /// The raw recorded data, regardless of variant — the text of a `Text` run or
    /// the markup of a `Placeholder`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text(data) | Self::Placeholder(data) => data,
        }
    }
}

/// A validated BCP-47 language tag.
///
/// Built via [`from_string`](LanguageTag::from_string) (parse a raw string) or
/// [`from_parsed`](LanguageTag::from_parsed) (reuse an already-parsed `oxilangtag`
/// tag). Both require a well-formed tag that includes a region subtag; script and
/// other subtags are optional. Validity is structural (grammar) only — there is
/// no registry lookup, so private-use tags such as `qaa-QM` are accepted.
///
/// The original case is preserved; identity treats tags case-insensitively (see
/// [`crate::identity`]), so `from_string("en-US")` and `from_string("en-us")` are
/// structurally distinct but share an `AtomId`. The validated `oxilangtag` value
/// is reachable through [`as_parsed`](LanguageTag::as_parsed) for callers that
/// need its subtag accessors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTag(OxiLanguageTag<String>);

impl LanguageTag {
    /// Parses a raw string into a validated tag.
    ///
    /// # Errors
    /// [`LanguageTagError::Malformed`] if `tag` is not a well-formed BCP-47 tag,
    /// or [`LanguageTagError::MissingRegion`] if it is well-formed but has no
    /// region subtag.
    pub fn from_string(tag: impl Into<String>) -> Result<Self, LanguageTagError> {
        let tag = tag.into();
        match OxiLanguageTag::parse(tag.clone()) {
            Ok(parsed) => Self::from_parsed(parsed),
            Err(error) => Err(LanguageTagError::Malformed { tag, error }),
        }
    }

    /// Validates an already-parsed `oxilangtag` tag, checking only the region
    /// requirement (the grammar is already guaranteed). Skips re-parsing for
    /// callers that already hold a parsed tag.
    ///
    /// # Errors
    /// [`LanguageTagError::MissingRegion`] if `parsed` has no region subtag.
    pub fn from_parsed(parsed: OxiLanguageTag<String>) -> Result<Self, LanguageTagError> {
        if parsed.region().is_some() {
            Ok(Self(parsed))
        } else {
            Err(LanguageTagError::MissingRegion {
                tag: parsed.as_str().to_owned(),
            })
        }
    }

    /// The tag as written — original case preserved.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The validated `oxilangtag` value, for access to its subtag accessors
    /// (`region()`, `script()`, …).
    pub fn as_parsed(&self) -> &OxiLanguageTag<String> {
        &self.0
    }
}

/// Why a [`LanguageTag`] failed to construct.
#[derive(Debug)]
pub enum LanguageTagError {
    /// The tag is not well-formed per the BCP-47 (RFC 5646) grammar.
    Malformed {
        /// The offending tag.
        tag: String,
        /// The underlying parser error.
        error: LanguageTagParseError,
    },
    /// The tag is well-formed but has no region subtag, which is required.
    MissingRegion {
        /// The offending tag.
        tag: String,
    },
}

impl fmt::Display for LanguageTagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { tag, error } => {
                write!(
                    f,
                    "{tag:?} is not a well-formed BCP-47 language tag: {error}"
                )
            }
            Self::MissingRegion { tag } => {
                write!(f, "{tag:?} is missing a required region subtag")
            }
        }
    }
}

impl std::error::Error for LanguageTagError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tag_from_string_accepts_correctly_formed_tags_and_stores_unchanged() {
        let tag = LanguageTag::from_string("en-US").unwrap();
        assert_eq!(tag.as_str(), "en-US");
    }

    #[test]
    fn language_tag_from_string_rejects_missing_region() {
        let error = LanguageTag::from_string("en").unwrap_err();
        assert!(matches!(error, LanguageTagError::MissingRegion { .. }));
    }

    #[test]
    fn language_tag_from_string_rejects_empty_tag() {
        let error = LanguageTag::from_string("").unwrap_err();
        assert!(matches!(error, LanguageTagError::Malformed { .. }));
    }

    #[test]
    fn language_tag_from_string_accepts_well_formed_but_invalid_region() {
        let tag = LanguageTag::from_string("en-WY").unwrap();
        assert_eq!(tag.as_str(), "en-WY");
    }

    #[test]
    fn language_tag_from_string_rejects_malformed_tags() {
        let error = LanguageTag::from_string("not a tag").unwrap_err();
        assert!(matches!(error, LanguageTagError::Malformed { .. }));
    }

    #[test]
    fn language_tag_from_parsed_accepts_correctly_formed_tags_and_stores_unchanged() {
        let tag =
            LanguageTag::from_parsed(OxiLanguageTag::parse("en-US".to_owned()).unwrap()).unwrap();
        assert_eq!(tag.as_str(), "en-US");
    }
    #[test]
    fn language_tag_from_parsed_rejects_missing_region() {
        let error =
            LanguageTag::from_parsed(OxiLanguageTag::parse("en".to_owned()).unwrap()).unwrap_err();
        assert!(matches!(error, LanguageTagError::MissingRegion { .. }));
    }

    #[test]
    fn language_tag_from_parsed_accepts_well_formed_but_invalid_region() {
        let tag =
            LanguageTag::from_parsed(OxiLanguageTag::parse("en-WY".to_owned()).unwrap()).unwrap();
        assert_eq!(tag.as_str(), "en-WY");
    }

    #[test]
    fn content_node_text_creates_text_node() {
        let text = ContentNode::text("hi");
        assert_eq!(text, ContentNode::Text("hi".to_owned()));
    }

    #[test]
    fn content_node_placeholder_creates_placeholder_node() {
        let placeholder = ContentNode::placeholder("<x/>");
        assert_eq!(placeholder, ContentNode::Placeholder("<x/>".to_owned()));
    }

    #[test]
    fn content_node_as_str_returns_data() {
        let text = ContentNode::text("hi");
        assert_eq!(text.as_str(), "hi");
    }

    #[test]
    fn atom_constructor_creates_atom() {
        let atom = Atom::new(
            LanguageTag::from_string("en-US").unwrap(),
            [ContentNode::text("hi")],
        );
        assert_eq!(atom.content(), &[ContentNode::text("hi")]);
    }

    #[test]
    fn atom_reconstructs_iterates_over_content() {
        let atom = Atom::new(
            LanguageTag::from_string("en-US").unwrap(),
            [
                ContentNode::text("hi"),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("bye"),
            ],
        );
        assert_eq!(atom.reconstruct(), "hi<x/>bye");
    }
}
