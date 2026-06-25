//! Parsing one XLIFF 1.2 content fragment into [`ContentNode`]s.
//!
//! [`parse_segment`] is a stateless tokenizer over the inline body of a
//! `<source>`/`<target>`; see its docs for the content model and entity handling.

use std::fmt;

use observatory_core::ir::ContentNode;
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesEnd, BytesStart, Event};

/// Inline elements whose content is native code — captured whole as one opaque
/// placeholder (we never look inside).
const CODE_CONTENT: &[&[u8]] = &[b"bpt", b"ept", b"ph", b"it"];
/// Inline elements whose content is translatable text — the tags become
/// placeholders and the inner text is kept.
const TEXT_CONTENT: &[&[u8]] = &[b"g", b"mrk"];
/// Empty inline elements — a single presence placeholder.
const EMPTY: &[&[u8]] = &[b"x", b"bx", b"ex"];

/// How text entity references are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityMode {
    /// Decode standard XML entities and numeric character references to their
    /// characters (`&amp;` → `&`). Round-trip is content-identical.
    Logical,
    /// Keep entity references as their raw bytes (`&amp;` stays `&amp;`).
    /// Round-trip is byte-identical.
    Verbatim,
}

/// Why a segment couldn't be parsed.
#[derive(Debug)]
pub enum XliffParseError {
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
    /// reference was found in text under [`EntityMode::Logical`].
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

impl fmt::Display for XliffParseError {
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

impl std::error::Error for XliffParseError {}

impl From<quick_xml::Error> for XliffParseError {
    fn from(error: quick_xml::Error) -> Self {
        Self::QuickXmlError { error }
    }
}

/// Tokenizes one XLIFF 1.2 content fragment into [`ContentNode`]s.
///
/// `content` is the inline body of a `<source>`/`<target>` (the wrapping element
/// already stripped by the caller), assumed well-formed. Translatable text becomes
/// [`ContentNode::Text`]; every inline element becomes an opaque
/// [`ContentNode::Placeholder`] holding its raw markup verbatim. Text, entities,
/// and CDATA are handled per `mode`; entities *inside* a placeholder are never
/// touched.
///
/// # Errors
/// [`XliffParseError::UnknownTag`] for an element outside the XLIFF 1.2 inline
/// set; [`XliffParseError::UnsupportedConstruct`] for a comment, processing
/// instruction, declaration, or doctype; [`XliffParseError::UnknownEntity`] for a
/// non-standard entity in text under [`EntityMode::Logical`]; or
/// [`XliffParseError::QuickXmlError`] if the reader fails.
pub fn parse_segment(content: &str, mode: EntityMode) -> Result<Vec<ContentNode>, XliffParseError> {
    let mut parser = XliffSegmentParser::new(content, mode);
    parser.run()?;
    Ok(parser.nodes)
}

/// True if `name` is one of the XLIFF 1.2 inline elements we recognize.
fn is_known_inline(name: &[u8]) -> bool {
    CODE_CONTENT.contains(&name) || TEXT_CONTENT.contains(&name) || EMPTY.contains(&name)
}

/// Builds an [`XliffParseError::UnknownTag`] from a raw element name.
fn unknown_tag(name: &[u8]) -> XliffParseError {
    XliffParseError::UnknownTag {
        tag: String::from_utf8_lossy(name).into_owned(),
    }
}

/// Labels an XML construct we don't accept in inline content, for a loud error.
fn unsupported(event: &Event) -> XliffParseError {
    let construct = match event {
        Event::Comment(_) => "comment",
        Event::PI(_) => "processing instruction",
        Event::Decl(_) => "XML declaration",
        Event::DocType(_) => "doctype",
        _ => "unexpected construct",
    };
    XliffParseError::UnsupportedConstruct {
        construct: construct.to_owned(),
    }
}

/// The mutable state threaded through one parse: the reader, the source it borrows
/// from, the chosen entity mode, the nodes collected so far, the text accumulated
/// since the last element boundary, and the byte offset just past what's consumed.
struct XliffSegmentParser<'a> {
    reader: Reader<&'a [u8]>,
    content: &'a str,
    mode: EntityMode,
    nodes: Vec<ContentNode>,
    pending: String,
    cursor: usize,
}

impl<'a> XliffSegmentParser<'a> {
    fn new(content: &'a str, mode: EntityMode) -> Self {
        Self {
            reader: Reader::from_str(content),
            content,
            mode,
            nodes: Vec::new(),
            pending: String::new(),
            cursor: 0,
        }
    }

    /// Reads events to end of input. Text and entities accumulate into `pending`;
    /// an element boundary flushes that text and emits the element's placeholder.
    fn run(&mut self) -> Result<(), XliffParseError> {
        loop {
            let event = self.reader.read_event()?;
            match event {
                Event::Text(_) => self.accumulate(),
                Event::GeneralRef(_) => self.accumulate_entity()?,
                Event::CData(_) => self.accumulate_cdata(),
                Event::Start(node) => self.handle_start(node)?,
                Event::End(node) => self.handle_end(node)?,
                Event::Empty(node) => self.handle_empty(node)?,
                Event::Eof => {
                    self.flush();
                    break;
                }
                // Anything else — comment, PI, declaration, doctype — has no
                // place in inline content. Fail loud rather than drop it.
                other => return Err(unsupported(&other)),
            }
        }
        Ok(())
    }

    /// An open tag. Code-content is swallowed whole as one placeholder; a
    /// text-content open tag becomes a placeholder and its inner content keeps
    /// flowing as later events.
    fn handle_start(&mut self, node: BytesStart<'a>) -> Result<(), XliffParseError> {
        let name = node.local_name();
        if CODE_CONTENT.contains(&name.as_ref()) {
            self.reader.read_to_end(node.name())?;
            self.push_placeholder();
        } else if TEXT_CONTENT.contains(&name.as_ref()) {
            self.push_placeholder();
        } else {
            return Err(unknown_tag(name.as_ref()));
        }
        Ok(())
    }

    /// A close tag. Only text-content closes reach here — code-content ends were
    /// already consumed by `read_to_end`.
    fn handle_end(&mut self, node: BytesEnd<'a>) -> Result<(), XliffParseError> {
        if TEXT_CONTENT.contains(&node.local_name().as_ref()) {
            self.push_placeholder();
            Ok(())
        } else {
            Err(unknown_tag(node.local_name().as_ref()))
        }
    }

    /// A self-closing inline element (`<x/>`, `<ph/>`, …): one presence
    /// placeholder, whatever its category.
    fn handle_empty(&mut self, node: BytesStart<'a>) -> Result<(), XliffParseError> {
        if is_known_inline(node.local_name().as_ref()) {
            self.push_placeholder();
            Ok(())
        } else {
            Err(unknown_tag(node.local_name().as_ref()))
        }
    }

    /// Appends an entity reference to the pending buffer — its raw bytes under
    /// [`EntityMode::Verbatim`], its decoded character under [`EntityMode::Logical`].
    fn accumulate_entity(&mut self) -> Result<(), XliffParseError> {
        let raw = self.take();
        match self.mode {
            EntityMode::Verbatim => self.pending.push_str(raw),
            EntityMode::Logical => {
                let decoded = unescape(raw).map_err(|_| XliffParseError::UnknownEntity {
                    entity: raw.to_owned(),
                })?;
                self.pending.push_str(&decoded);
            }
        }
        Ok(())
    }

    /// Appends a CDATA section to the pending buffer. CDATA is character data, so
    /// it follows the mode like text and entities: its raw `<![CDATA[…]]>` bytes
    /// under [`EntityMode::Verbatim`], its unwrapped content under
    /// [`EntityMode::Logical`]. The content is never markup- or entity-parsed —
    /// inside CDATA there are none — so logical only strips the delimiters.
    fn accumulate_cdata(&mut self) {
        let raw = self.take();
        match self.mode {
            EntityMode::Verbatim => self.pending.push_str(raw),
            EntityMode::Logical => {
                let inner = &raw["<![CDATA[".len()..raw.len() - "]]>".len()];
                self.pending.push_str(inner);
            }
        }
    }

    /// Appends the current text run to the pending buffer (raw bytes).
    fn accumulate(&mut self) {
        let raw = self.take();
        self.pending.push_str(raw);
    }

    /// Flushes any accumulated text as a single [`ContentNode::Text`].
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            self.nodes
                .push(ContentNode::text(std::mem::take(&mut self.pending)));
        }
    }

    /// Flushes any pending text, then records everything since the cursor as a
    /// placeholder node — so the text before this element lands before it.
    fn push_placeholder(&mut self) {
        self.flush();
        let raw = self.take();
        self.nodes.push(ContentNode::placeholder(raw));
    }

    /// The raw source from the cursor up to the reader's current position,
    /// advancing the cursor past it. The slice borrows the original `content`, so
    /// it outlives the `&mut self` borrow.
    fn take(&mut self) -> &'a str {
        let content = self.content;
        let end = usize::try_from(self.reader.buffer_position()).expect("position fits usize");
        let raw = &content[self.cursor..end];
        self.cursor = end;
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses under `Logical`, asserting success.
    fn logical(content: &str) -> Vec<ContentNode> {
        parse_segment(content, EntityMode::Logical).unwrap()
    }

    /// Parses under `Verbatim`, asserting success.
    fn verbatim(content: &str) -> Vec<ContentNode> {
        parse_segment(content, EntityMode::Verbatim).unwrap()
    }

    fn text(data: &str) -> ContentNode {
        ContentNode::text(data)
    }

    fn placeholder(data: &str) -> ContentNode {
        ContentNode::placeholder(data)
    }

    #[test]
    fn empty_input_yields_no_nodes() {
        assert!(logical("").is_empty());
    }

    #[test]
    fn plain_text_is_one_text_node() {
        assert_eq!(logical("just words"), [text("just words")]);
    }

    #[test]
    fn code_content_is_one_opaque_placeholder() {
        assert_eq!(
            logical(r#"Click <ph id="1">{0}</ph> here"#),
            [
                text("Click "),
                placeholder(r#"<ph id="1">{0}</ph>"#),
                text(" here"),
            ]
        );
    }

    #[test]
    fn paired_code_content_each_captured_whole() {
        assert_eq!(
            logical(r#"a<bpt id="1">b</bpt>c<ept id="1">d</ept>e"#),
            [
                text("a"),
                placeholder(r#"<bpt id="1">b</bpt>"#),
                text("c"),
                placeholder(r#"<ept id="1">d</ept>"#),
                text("e"),
            ]
        );
    }

    #[test]
    fn text_content_keeps_inner_text_tags_as_placeholders() {
        assert_eq!(
            logical(r#"<g id="1">x</g>"#),
            [placeholder(r#"<g id="1">"#), text("x"), placeholder("</g>")]
        );
    }

    #[test]
    fn nested_text_content_with_code_inside() {
        assert_eq!(
            logical(r#"<g id="1">a<ph id="1">x</ph>b<g id="2">c</g>d</g>"#),
            [
                placeholder(r#"<g id="1">"#),
                text("a"),
                placeholder(r#"<ph id="1">x</ph>"#),
                text("b"),
                placeholder(r#"<g id="2">"#),
                text("c"),
                placeholder("</g>"),
                text("d"),
                placeholder("</g>"),
            ]
        );
    }

    #[test]
    fn empty_element_is_one_placeholder() {
        assert_eq!(
            logical("before<x/>after"),
            [text("before"), placeholder("<x/>"), text("after")]
        );
    }

    #[test]
    fn self_closed_code_element_is_one_placeholder() {
        assert_eq!(logical(r#"<ph id="1"/>"#), [placeholder(r#"<ph id="1"/>"#)]);
    }

    #[test]
    fn logical_decodes_entities_and_merges_runs() {
        assert_eq!(logical("Tom &amp; Jerry &lt;3"), [text("Tom & Jerry <3")]);
    }

    #[test]
    fn verbatim_keeps_entities_raw() {
        assert_eq!(
            verbatim("Tom &amp; Jerry &lt;3"),
            [text("Tom &amp; Jerry &lt;3")]
        );
    }

    #[test]
    fn logical_decodes_numeric_character_references() {
        assert_eq!(logical("A&#66;C"), [text("ABC")]);
    }

    #[test]
    fn entities_inside_placeholders_are_never_decoded() {
        let raw = r#"a<ph id="1">&amp;</ph>b"#;
        let expected = [
            text("a"),
            placeholder(r#"<ph id="1">&amp;</ph>"#),
            text("b"),
        ];
        assert_eq!(logical(raw), expected);
        assert_eq!(verbatim(raw), expected);
    }

    #[test]
    fn cdata_logical_strips_delimiters_without_unescaping() {
        // The &amp; inside CDATA is literal — logical only removes the delimiters.
        assert_eq!(
            logical("a<![CDATA[b &amp; <c>]]>d"),
            [text("ab &amp; <c>d")]
        );
    }

    #[test]
    fn cdata_verbatim_keeps_the_section_raw() {
        assert_eq!(
            verbatim("a<![CDATA[b &amp; <c>]]>d"),
            [text("a<![CDATA[b &amp; <c>]]>d")]
        );
    }

    #[test]
    fn unknown_entity_under_logical_errors() {
        let error = parse_segment("plain &nbsp; text", EntityMode::Logical).unwrap_err();
        assert!(matches!(error, XliffParseError::UnknownEntity { .. }));
    }

    #[test]
    fn unknown_entity_under_verbatim_is_kept_raw() {
        assert_eq!(verbatim("plain &nbsp; text"), [text("plain &nbsp; text")]);
    }

    #[test]
    fn unknown_element_errors() {
        let error = parse_segment("<foo>x</foo>", EntityMode::Logical).unwrap_err();
        assert!(matches!(error, XliffParseError::UnknownTag { .. }));
    }

    #[test]
    fn comment_errors_loudly() {
        let error = parse_segment("a<!-- nope -->b", EntityMode::Logical).unwrap_err();
        assert!(matches!(
            error,
            XliffParseError::UnsupportedConstruct { .. }
        ));
    }
}
