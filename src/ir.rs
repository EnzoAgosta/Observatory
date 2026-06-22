//! The data model: an [`Atom`] and the pieces it is built from.
//!
//! An [`Atom`] is an ordered sequence of [`ContentNode`]s, each either
//! translatable text or an opaque placeholder standing in for non-text (an
//! inline tag, a variable, a code). Recording an atom is faithful: it stores
//! exactly the nodes it is given, and the original string is recovered by joining
//! every node's data in order.
//!
//! An atom's *identity* is a separate, normalized projection (see
//! [`crate::identity`]), so two atoms can be structurally different yet share an
//! identity. Atoms are compared for identity through their `AtomId`, not with
//! `==`.

use oxilangtag::LanguageTag as ParsedTag;
use std::fmt;

/// A content-addressed, single-language string.
///
/// The order of [`content`](Atom::content) is significant. Construction is
/// faithful — an `Atom` preserves exactly the nodes it was built from, including
/// adjacent and empty text runs — so `==` means "structurally identical," not
/// "same identity." Identity is the `AtomId` (see [`crate::identity`]); two atoms
/// that differ only in how their text was split share an `AtomId` without being
/// `==`.
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
        self.content.iter().map(|node| node.data.as_str()).collect()
    }
}

/// One run of an [`Atom`]: either translatable text or an opaque placeholder.
///
/// `data` is the raw recorded content and is never interpreted. For a placeholder
/// it is the original markup (an inline tag, a variable, …); for text it is the
/// text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentNode {
    is_placeholder: bool,
    data: String,
}

impl ContentNode {
    /// A translatable text run.
    pub fn text(data: impl Into<String>) -> Self {
        Self {
            is_placeholder: false,
            data: data.into(),
        }
    }

    /// An opaque placeholder standing in for non-text; `data` is its raw,
    /// uninterpreted content.
    pub fn placeholder(data: impl Into<String>) -> Self {
        Self {
            is_placeholder: true,
            data: data.into(),
        }
    }

    /// Whether this node is a placeholder (`true`) or translatable text
    /// (`false`).
    pub fn is_placeholder(&self) -> bool {
        self.is_placeholder
    }

    /// The raw recorded content of this node.
    pub fn data(&self) -> &str {
        &self.data
    }
}

/// A validated BCP-47 language tag.
///
/// Constructed via [`LanguageTag::parse`], which requires a well-formed tag that
/// includes a region (script and other subtags are optional). The original case
/// is preserved; identity treats tags case-insensitively (see
/// [`crate::identity`]), so `parse("en-US")` and `parse("en-us")` are structurally
/// distinct but share an `AtomId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTag(ParsedTag<String>);

impl LanguageTag {
    /// Parses and validates a BCP-47 tag. The tag must be well-formed and include
    /// a region subtag; script and other subtags are optional. Validity is
    /// structural (grammar) only — there is no registry lookup, so private-use
    /// tags such as `qaa-QM` are accepted.
    ///
    /// # Errors
    /// [`LanguageTagError::Malformed`] if `tag` is not a well-formed BCP-47 tag,
    /// or [`LanguageTagError::MissingRegion`] if it is well-formed but has no
    /// region subtag.
    pub fn parse(tag: impl Into<String>) -> Result<Self, LanguageTagError> {
        let tag = tag.into();
        match ParsedTag::parse(tag.clone()) {
            Ok(parsed) if parsed.region().is_some() => Ok(Self(parsed)),
            Ok(_) => Err(LanguageTagError::MissingRegion { tag }),
            Err(_) => Err(LanguageTagError::Malformed { tag }),
        }
    }

    /// The tag as written — original case preserved.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Why a [`LanguageTag`] failed to [`parse`](LanguageTag::parse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageTagError {
    /// The tag is not well-formed per the BCP-47 (RFC 5646) grammar.
    Malformed {
        /// The offending tag.
        tag: String,
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
            Self::Malformed { tag } => {
                write!(f, "not a well-formed BCP-47 language tag: {tag:?}")
            }
            Self::MissingRegion { tag } => {
                write!(
                    f,
                    "language tag {tag:?} is missing a required region subtag"
                )
            }
        }
    }
}

impl std::error::Error for LanguageTagError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang() -> LanguageTag {
        LanguageTag::parse("en-us").unwrap()
    }

    #[test]
    fn new_preserves_text_runs_faithfully() {
        // Adjacent and empty text runs are kept as-is; normalization happens
        // later, when identity is computed.
        let nodes = [
            ContentNode::text("Hello, "),
            ContentNode::text(""),
            ContentNode::text("world"),
        ];
        let atom = Atom::new(lang(), nodes.clone());
        assert_eq!(atom.content(), nodes.as_slice());
    }

    #[test]
    fn new_preserves_placeholders_in_order() {
        let nodes = [
            ContentNode::text("a"),
            ContentNode::placeholder("<x/>"),
            ContentNode::placeholder(""),
            ContentNode::text("b"),
        ];
        let atom = Atom::new(lang(), nodes.clone());
        assert_eq!(atom.content(), nodes.as_slice());
    }

    #[test]
    fn reconstruct_joins_data_in_order() {
        let atom = Atom::new(
            lang(),
            [
                ContentNode::text("Click "),
                ContentNode::placeholder("<g id=1>"),
                ContentNode::text("here"),
                ContentNode::placeholder("</g>"),
            ],
        );
        assert_eq!(atom.reconstruct(), "Click <g id=1>here</g>");
    }

    #[test]
    fn distinct_chunkings_are_unequal_but_reconstruct_alike() {
        // Different chunkings are distinct atoms (structural !=) yet reconstruct
        // identically — the property identity turns into equal ids.
        let chunked = Atom::new(lang(), [ContentNode::text("a"), ContentNode::text("b")]);
        let merged = Atom::new(lang(), [ContentNode::text("ab")]);
        assert_ne!(chunked, merged);
        assert_eq!(chunked.reconstruct(), merged.reconstruct());
    }

    #[test]
    fn parse_accepts_language_and_region() {
        assert_eq!(LanguageTag::parse("en-US").unwrap().as_str(), "en-US");
    }

    #[test]
    fn parse_preserves_case_faithfully() {
        // No normalization at construction; lowercasing happens at identity time.
        assert_eq!(LanguageTag::parse("EN-us").unwrap().as_str(), "EN-us");
    }

    #[test]
    fn parse_requires_a_region() {
        assert_eq!(
            LanguageTag::parse("en"),
            Err(LanguageTagError::MissingRegion {
                tag: "en".to_string()
            })
        );
    }

    #[test]
    fn parse_rejects_malformed_tags() {
        assert!(matches!(
            LanguageTag::parse("not a tag"),
            Err(LanguageTagError::Malformed { .. })
        ));
        // Underscores are not well-formed BCP-47 (hyphen-only).
        assert!(matches!(
            LanguageTag::parse("en_US"),
            Err(LanguageTagError::Malformed { .. })
        ));
    }

    #[test]
    fn parse_script_is_optional() {
        assert!(LanguageTag::parse("sr-RS").is_ok()); // no script
        assert!(LanguageTag::parse("sr-Cyrl-RS").is_ok()); // with script
    }

    #[test]
    fn parse_accepts_well_formed_private_use_tags() {
        // Well-formedness only, no registry lookup.
        assert!(LanguageTag::parse("qaa-QM").is_ok());
    }

    #[test]
    fn language_tag_equality_is_case_sensitive() {
        // Faithful storage: structurally distinct now, same identity later.
        assert_ne!(
            LanguageTag::parse("en-US").unwrap(),
            LanguageTag::parse("en-us").unwrap()
        );
    }

    #[test]
    fn content_node_accessors() {
        let text = ContentNode::text("hi");
        assert!(!text.is_placeholder());
        assert_eq!(text.data(), "hi");

        let placeholder = ContentNode::placeholder("<x/>");
        assert!(placeholder.is_placeholder());
        assert_eq!(placeholder.data(), "<x/>");
    }
}
