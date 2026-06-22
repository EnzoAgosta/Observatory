//! The intermediate representation (IR): the canonical data model for an
//! [`Atom`] — a content-addressed, single-language string distilled from XLIFF
//! 1.2 but independent of it (D3).
//!
//! An [`Atom`] is an ordered sequence of [`ContentNode`]s, each either
//! translatable text or an opaque [placeholder](ContentKind::Placeholder)
//! standing in for non-text. Building an `Atom` is a dumb, reversible recording
//! of "what is text and what is placeholder" (D14, D16): the IR never interprets
//! a placeholder's contents, and reconstructing the original string is simply
//! the in-order join of every node's data.
//!
//! Identity (the `AtomId`) is a *separate* normalized projection over this model
//! and arrives in later sub-phases; see `docs/DECISIONS.md` (Phase 1b–1c).

/// A content-addressed, single-language string.
///
/// The order of [`content`](Atom::content) is significant. An `Atom` is always
/// in canonical form: [`Atom::new`] merges adjacent text runs and drops empty
/// ones, so two equal `Atom`s have identical node sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    language: LanguageTag,
    content: Vec<ContentNode>,
}

impl Atom {
    /// Records `nodes` into a canonical `Atom`.
    ///
    /// Canonicalization (D14, D16) is intentionally minimal: adjacent
    /// [`Text`](ContentKind::Text) nodes are merged and empty `Text` nodes are
    /// dropped. Placeholders are recorded verbatim and never merged — their
    /// count and position carry meaning.
    pub fn new(language: LanguageTag, nodes: impl IntoIterator<Item = ContentNode>) -> Self {
        let mut content: Vec<ContentNode> = Vec::new();
        for node in nodes {
            if node.kind == ContentKind::Text {
                if node.data.is_empty() {
                    continue;
                }
                if let Some(last) = content.last_mut()
                    && last.kind == ContentKind::Text
                {
                    last.data.push_str(&node.data);
                    continue;
                }
            }
            content.push(node);
        }
        Self { language, content }
    }

    /// The language of this atom.
    pub fn language(&self) -> &LanguageTag {
        &self.language
    }

    /// The atom's content nodes, in significant order.
    pub fn content(&self) -> &[ContentNode] {
        &self.content
    }

    /// Reconstructs the original recorded string: the in-order join of every
    /// node's raw data.
    ///
    /// This is the reversible half of the recording (D14) and is independent of
    /// identity; it is invariant under canonicalization.
    pub fn reconstruct(&self) -> String {
        self.content.iter().map(|node| node.data.as_str()).collect()
    }
}

/// One run of an [`Atom`]: either translatable text or an opaque placeholder.
///
/// `data` is the raw recorded content and is never interpreted (D14, D16). For a
/// placeholder it is the opaque original markup; for text it is the text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentNode {
    kind: ContentKind,
    data: String,
}

impl ContentNode {
    /// A translatable text run.
    pub fn text(data: impl Into<String>) -> Self {
        Self {
            kind: ContentKind::Text,
            data: data.into(),
        }
    }

    /// An opaque placeholder standing in for non-text; `data` is its raw,
    /// uninterpreted original content.
    pub fn placeholder(data: impl Into<String>) -> Self {
        Self {
            kind: ContentKind::Placeholder,
            data: data.into(),
        }
    }

    /// Whether this node is text or a placeholder.
    pub fn kind(&self) -> ContentKind {
        self.kind
    }

    /// The raw recorded content of this node.
    pub fn data(&self) -> &str {
        &self.data
    }
}

/// Distinguishes the two — and only two — kinds of content node (D16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    /// Translatable text.
    Text,
    /// An opaque stand-in for non-text (a tag, variable, code, ...).
    Placeholder,
}

/// A BCP-47 language tag.
///
/// A thin, unvalidated newtype for now. Real handling — lowercase
/// canonicalization, a mandatory region subtag, and well-formedness via
/// `oxilangtag` (D7, D11) — wires in at Phase 1d.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTag(String);

impl LanguageTag {
    /// Wraps a raw tag string. No validation yet (Phase 1d).
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    /// The raw tag string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang() -> LanguageTag {
        LanguageTag::new("en-us")
    }

    #[test]
    fn merges_adjacent_text() {
        let atom = Atom::new(
            lang(),
            [ContentNode::text("Hello, "), ContentNode::text("world")],
        );
        assert_eq!(
            atom.content(),
            [ContentNode::text("Hello, world")].as_slice()
        );
    }

    #[test]
    fn drops_empty_text() {
        let atom = Atom::new(
            lang(),
            [
                ContentNode::text(""),
                ContentNode::text("x"),
                ContentNode::text(""),
            ],
        );
        assert_eq!(atom.content(), [ContentNode::text("x")].as_slice());
    }

    #[test]
    fn placeholder_between_text_blocks_merge() {
        let nodes = [
            ContentNode::text("a"),
            ContentNode::placeholder("<x/>"),
            ContentNode::text("b"),
        ];
        let atom = Atom::new(lang(), nodes.clone());
        assert_eq!(atom.content(), nodes.as_slice());
    }

    #[test]
    fn adjacent_placeholders_are_not_merged() {
        let atom = Atom::new(
            lang(),
            [
                ContentNode::placeholder("<x id=1/>"),
                ContentNode::placeholder("<x id=2/>"),
            ],
        );
        assert_eq!(atom.content().len(), 2);
    }

    #[test]
    fn empty_placeholder_is_preserved() {
        let atom = Atom::new(lang(), [ContentNode::placeholder("")]);
        assert_eq!(atom.content(), [ContentNode::placeholder("")].as_slice());
    }

    #[test]
    fn reconstruct_is_invariant_under_canonicalization() {
        let raw = [
            ContentNode::text("Click "),
            ContentNode::text(""),
            ContentNode::placeholder("<g id=1>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ];
        let joined: String = raw.iter().map(ContentNode::data).collect();
        let atom = Atom::new(lang(), raw);
        assert_eq!(atom.reconstruct(), joined);
        assert_eq!(atom.reconstruct(), "Click <g id=1>here</g>");
    }

    #[test]
    fn language_tag_preserves_its_raw_string() {
        let tag = LanguageTag::new("en-us");
        assert_eq!(tag.as_str(), "en-us");
    }

    #[test]
    fn content_node_accessors() {
        let text = ContentNode::text("hi");
        assert_eq!(text.kind(), ContentKind::Text);
        assert_eq!(text.data(), "hi");

        let placeholder = ContentNode::placeholder("<x/>");
        assert_eq!(placeholder.kind(), ContentKind::Placeholder);
        assert_eq!(placeholder.data(), "<x/>");
    }
}
