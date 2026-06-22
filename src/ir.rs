//! The intermediate representation (IR): the canonical data model for an
//! [`Atom`] — a content-addressed, single-language string distilled from XLIFF
//! 1.2 but independent of it (D3).
//!
//! An [`Atom`] is an ordered sequence of [`ContentNode`]s, each either
//! translatable text or an opaque [placeholder](ContentNode::placeholder)
//! standing in for non-text. Building an `Atom` is a faithful, non-normalizing
//! recording (D14, D17): [`Atom::new`] stores exactly the nodes it is given, and
//! reconstructing the original string is the in-order join of every node's data.
//!
//! Normalization for identity — merging adjacent text, dropping empty runs — is
//! *not* done here; it lives in the `AtomId` computation (Phase 1b–1c). As a
//! result two structurally different `Atom`s can share an `AtomId`: structural
//! equality (`==`) is a different relation from identity. **Dedup and identity
//! are always via `AtomId`, never `==`.** See `docs/DECISIONS.md` (D16, D17).

/// A content-addressed, single-language string.
///
/// The order of [`content`](Atom::content) is significant. Construction is
/// faithful: an `Atom` preserves exactly the nodes it was built from (adjacent
/// and empty text runs included), so `==` means "structurally identical
/// recording" — *not* "same identity." Identity is the `AtomId`, a normalized
/// projection computed separately (Phase 1b–1c); two `Atom`s that differ only in
/// incidental text chunking share an `AtomId` without being `==`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    language: LanguageTag,
    content: Vec<ContentNode>,
}

impl Atom {
    /// Records `nodes` into an `Atom` faithfully — exactly as given, with no
    /// merging, dropping, or reordering (D14, D17).
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

    /// The atom's content nodes, in significant order.
    pub fn content(&self) -> &[ContentNode] {
        &self.content
    }

    /// Reconstructs the original recorded string: the in-order join of every
    /// node's raw data (the reversible half of the recording, D14).
    pub fn reconstruct(&self) -> String {
        self.content.iter().map(|node| node.data.as_str()).collect()
    }
}

/// One run of an [`Atom`]: either translatable text or an opaque placeholder
/// (the closed binary distinction of D16).
///
/// `data` is the raw recorded content and is never interpreted (D14, D16). For a
/// placeholder it is the opaque original markup; for text it is the text run.
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
    /// uninterpreted original content.
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
    fn new_preserves_text_runs_faithfully() {
        // Adjacent and empty text runs are NOT merged or dropped (D17): the
        // recording is faithful; normalization happens only in the AtomId.
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
        // Different chunkings are distinct Atoms (structural !=) yet reconstruct
        // identically — the property the AtomId will turn into equal ids (D17).
        let chunked = Atom::new(lang(), [ContentNode::text("a"), ContentNode::text("b")]);
        let merged = Atom::new(lang(), [ContentNode::text("ab")]);
        assert_ne!(chunked, merged);
        assert_eq!(chunked.reconstruct(), merged.reconstruct());
    }

    #[test]
    fn language_tag_preserves_its_raw_string() {
        let tag = LanguageTag::new("en-us");
        assert_eq!(tag.as_str(), "en-us");
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
