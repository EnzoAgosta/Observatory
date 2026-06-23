//! Parsing an XLIFF 1.2 content fragment into an [`Atom`].
//!
//! The fragment is the *body* of a `<source>` / `<target>` — the inline content,
//! not the wrapping element. The language is supplied by the caller (it lives a
//! level up, on `<file>`), which also gates the caller's own validity.
//!
//! The tokenizer is a single forward pass over the XML events; the structure of
//! placeholder pairing is never reconstructed, because identity cares only about
//! placeholder *position and count* (D16). Each element is classified solely by
//! what XLIFF 1.2 says its content is:
//!
//! - **native code** (`<bpt>`, `<ept>`, `<ph>`, `<it>`): the whole element,
//!   including any nested `<sub>`, is read to its end and recorded as one opaque
//!   placeholder.
//! - **translatable text** (`<g>`, `<mrk>`): the open tag and close tag each
//!   become a placeholder, and the inner content is tokenized as text.
//! - **empty inline** (`<x/>`, `<bx/>`, `<ex/>`): a single placeholder.
//!
//! Placeholders are recorded as the raw bytes of the input, so attribute order
//! and quoting survive untouched.

use crate::codec::{Codec, EntityMode};
use crate::error::ParseError;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Parses an XLIFF 1.2 content fragment into an [`Atom`] in `language`, treating
/// entities per `codec`.
///
/// `content` is the inline body of a content node (e.g. what sits between
/// `<source>` and `</source>`). The fragment is assumed to be well-formed,
/// already-extracted XLIFF; structural or entity problems surface as a
/// [`ParseError`].
///
/// # Errors
/// See [`ParseError`]: malformed XML, an unknown named entity under
/// [`EntityMode::Logical`], a non-inline element, or an unsupported XML construct.
pub fn parse(content: &str, language: LanguageTag, codec: Codec) -> Result<Atom, ParseError> {
    let mut reader = Reader::from_str(content);
    let mut nodes: Vec<ContentNode> = Vec::new();
    let mut run = TextRun::new(codec.entities, content);

    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| ParseError::Malformed {
            detail: e.to_string(),
        })?;
        let end = reader.buffer_position() as usize;

        match event {
            Event::Eof => break,

            Event::Text(text) => {
                run.note(start, end);
                if codec.entities == EntityMode::Logical {
                    let decoded = text.decode().map_err(|e| ParseError::Malformed {
                        detail: e.to_string(),
                    })?;
                    run.push_str(&decoded);
                }
            }

            Event::CData(cdata) => {
                run.note(start, end);
                if codec.entities == EntityMode::Logical {
                    let decoded =
                        std::str::from_utf8(&cdata).map_err(|e| ParseError::Malformed {
                            detail: e.to_string(),
                        })?;
                    run.push_str(decoded);
                }
            }

            Event::GeneralRef(reference) => {
                run.note(start, end);
                if codec.entities == EntityMode::Logical {
                    resolve_reference(&reference, &mut run)?;
                }
            }

            Event::Start(tag) => {
                run.flush(&mut nodes);
                match classify(tag.local_name().as_ref()) {
                    Some(Inline::Code) => {
                        reader
                            .read_to_end(tag.name())
                            .map_err(|e| ParseError::Malformed {
                                detail: e.to_string(),
                            })?;
                        let element_end = reader.buffer_position() as usize;
                        nodes.push(ContentNode::placeholder(&content[start..element_end]));
                    }
                    Some(Inline::Text) => {
                        nodes.push(ContentNode::placeholder(&content[start..end]));
                    }
                    None => return Err(unexpected_element(tag.local_name().as_ref())),
                }
            }

            Event::Empty(tag) => {
                run.flush(&mut nodes);
                if classify(tag.local_name().as_ref()).is_some() {
                    nodes.push(ContentNode::placeholder(&content[start..end]));
                } else {
                    return Err(unexpected_element(tag.local_name().as_ref()));
                }
            }

            Event::End(tag) => {
                run.flush(&mut nodes);
                match classify(tag.local_name().as_ref()) {
                    Some(Inline::Text) => {
                        nodes.push(ContentNode::placeholder(&content[start..end]));
                    }
                    _ => return Err(unexpected_element(tag.local_name().as_ref())),
                }
            }

            Event::Comment(_) => return Err(unsupported("comment")),
            Event::PI(_) => return Err(unsupported("processing instruction")),
            Event::Decl(_) => return Err(unsupported("XML declaration")),
            Event::DocType(_) => return Err(unsupported("doctype")),
        }
    }

    run.flush(&mut nodes);
    Ok(Atom::new(language, nodes))
}

/// What an XLIFF 1.2 inline element's content model is — the only distinction the
/// tokenizer draws.
enum Inline {
    /// Content is native code; the whole element is one opaque placeholder.
    Code,
    /// Content is translatable text; the tags bracket text that is kept.
    Text,
}

/// Classifies an inline element by local name, or `None` if it is not an XLIFF
/// 1.2 inline element. Empty inline elements (`x`, `bx`, `ex`) report `Text` —
/// they have no content, so the distinction is moot, but it marks them as known.
fn classify(local_name: &[u8]) -> Option<Inline> {
    match local_name {
        b"bpt" | b"ept" | b"ph" | b"it" => Some(Inline::Code),
        b"g" | b"mrk" | b"x" | b"bx" | b"ex" => Some(Inline::Text),
        _ => None,
    }
}

/// Resolves a general reference into the logical text run: a numeric character
/// reference to its character, a predefined entity to its replacement, and
/// anything else to a hard error.
fn resolve_reference(
    reference: &quick_xml::events::BytesRef<'_>,
    run: &mut TextRun<'_>,
) -> Result<(), ParseError> {
    if let Some(character) = reference
        .resolve_char_ref()
        .map_err(|e| ParseError::Malformed {
            detail: e.to_string(),
        })?
    {
        run.push_char(character);
        return Ok(());
    }

    let name = reference.decode().map_err(|e| ParseError::Malformed {
        detail: e.to_string(),
    })?;
    match resolve_predefined_entity(&name) {
        Some(replacement) => {
            run.push_str(replacement);
            Ok(())
        }
        None => Err(ParseError::UnknownEntity {
            entity: name.into_owned(),
        }),
    }
}

fn unexpected_element(local_name: &[u8]) -> ParseError {
    ParseError::UnexpectedElement {
        name: String::from_utf8_lossy(local_name).into_owned(),
    }
}

fn unsupported(construct: &str) -> ParseError {
    ParseError::UnsupportedConstruct {
        construct: construct.to_owned(),
    }
}

/// Accumulates a run of text across consecutive text, CDATA, and reference events,
/// flushing it as a single [`ContentNode`] when a placeholder or the end is
/// reached.
///
/// In [`EntityMode::Verbatim`] only the input byte span is tracked, so the run is
/// emitted exactly as written. In [`EntityMode::Logical`] the decoded characters
/// are accumulated instead.
struct TextRun<'a> {
    mode: EntityMode,
    source: &'a str,
    span_start: usize,
    span_end: usize,
    started: bool,
    logical: String,
}

impl<'a> TextRun<'a> {
    fn new(mode: EntityMode, source: &'a str) -> Self {
        Self {
            mode,
            source,
            span_start: 0,
            span_end: 0,
            started: false,
            logical: String::new(),
        }
    }

    /// Records that the byte range `start..end` of the input belongs to the
    /// current run, extending it (or starting it).
    fn note(&mut self, start: usize, end: usize) {
        if !self.started {
            self.span_start = start;
            self.started = true;
        }
        self.span_end = end;
    }

    fn push_str(&mut self, text: &str) {
        self.logical.push_str(text);
    }

    fn push_char(&mut self, character: char) {
        self.logical.push(character);
    }

    /// Emits the accumulated run as a text node (if any) and resets, ready for the
    /// next run.
    fn flush(&mut self, nodes: &mut Vec<ContentNode>) {
        if !self.started {
            return;
        }
        let node = match self.mode {
            EntityMode::Verbatim => ContentNode::text(&self.source[self.span_start..self.span_end]),
            EntityMode::Logical => ContentNode::text(std::mem::take(&mut self.logical)),
        };
        nodes.push(node);
        self.started = false;
        self.logical.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> LanguageTag {
        LanguageTag::parse("en-us").unwrap()
    }

    fn parse_logical(content: &str) -> Result<Atom, ParseError> {
        parse(content, en(), Codec::logical())
    }

    /// Asserts an atom's nodes, each as (is_placeholder, data).
    fn assert_nodes(atom: &Atom, expected: &[(bool, &str)]) {
        let actual: Vec<(bool, &str)> = atom
            .content()
            .iter()
            .map(|node| (node.is_placeholder(), node.data()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn plain_text_is_one_text_node() {
        let atom = parse_logical("Hello, world").unwrap();
        assert_nodes(&atom, &[(false, "Hello, world")]);
    }

    #[test]
    fn empty_fragment_has_no_nodes() {
        let atom = parse_logical("").unwrap();
        assert!(atom.content().is_empty());
    }

    #[test]
    fn paired_g_brackets_kept_text() {
        // <g> wraps translatable text: open/close become placeholders, "here"
        // stays as text.
        let atom = parse_logical(r#"Click <g id="1">here</g>"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (false, "Click "),
                (true, r#"<g id="1">"#),
                (false, "here"),
                (true, "</g>"),
            ],
        );
    }

    #[test]
    fn nested_g_records_each_boundary_in_order() {
        let atom = parse_logical(r#"<g id="1">a<g id="2">b</g>c</g>"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (true, r#"<g id="1">"#),
                (false, "a"),
                (true, r#"<g id="2">"#),
                (false, "b"),
                (true, "</g>"),
                (false, "c"),
                (true, "</g>"),
            ],
        );
    }

    #[test]
    fn code_element_is_one_opaque_placeholder() {
        // <ph> content is native code; the whole element is hidden from the atom.
        let atom = parse_logical(r#"a<ph id="1">&lt;br/&gt;</ph>b"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (false, "a"),
                (true, r#"<ph id="1">&lt;br/&gt;</ph>"#),
                (false, "b"),
            ],
        );
    }

    #[test]
    fn bpt_ept_are_each_one_placeholder() {
        let atom =
            parse_logical(r#"<bpt id="1">&lt;b&gt;</bpt>x<ept id="1">&lt;/b&gt;</ept>"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (true, r#"<bpt id="1">&lt;b&gt;</bpt>"#),
                (false, "x"),
                (true, r#"<ept id="1">&lt;/b&gt;</ept>"#),
            ],
        );
    }

    #[test]
    fn sub_inside_code_stays_opaque() {
        // <sub> carries translatable text, but it is inside native code, so the
        // whole <ph> (sub and all) is one opaque placeholder (D8 / D26).
        let atom = parse_logical(r#"<ph id="1">img(<sub>alt text</sub>)</ph>"#).unwrap();
        assert_nodes(
            &atom,
            &[(true, r#"<ph id="1">img(<sub>alt text</sub>)</ph>"#)],
        );
    }

    #[test]
    fn empty_inline_elements_are_single_placeholders() {
        let atom = parse_logical(r#"a<x id="1"/>b<bx id="2"/>c<ex id="2"/>"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (false, "a"),
                (true, r#"<x id="1"/>"#),
                (false, "b"),
                (true, r#"<bx id="2"/>"#),
                (false, "c"),
                (true, r#"<ex id="2"/>"#),
            ],
        );
    }

    #[test]
    fn mrk_is_treated_like_g_for_now() {
        let atom = parse_logical(r#"<mrk mtype="term">CPU</mrk>"#).unwrap();
        assert_nodes(
            &atom,
            &[
                (true, r#"<mrk mtype="term">"#),
                (false, "CPU"),
                (true, "</mrk>"),
            ],
        );
    }

    #[test]
    fn logical_mode_decodes_predefined_entities() {
        let atom = parse_logical("Tom &amp; Jerry &lt;3").unwrap();
        assert_nodes(&atom, &[(false, "Tom & Jerry <3")]);
    }

    #[test]
    fn logical_mode_decodes_numeric_character_references() {
        let atom = parse_logical("A&#38;B&#x26;C").unwrap();
        assert_nodes(&atom, &[(false, "A&B&C")]);
    }

    #[test]
    fn verbatim_mode_keeps_entities_raw() {
        let atom = parse("Tom &amp; Jerry", en(), Codec::verbatim()).unwrap();
        assert_nodes(&atom, &[(false, "Tom &amp; Jerry")]);
    }

    #[test]
    fn verbatim_mode_accepts_unknown_entities() {
        // Verbatim never resolves, so a custom entity is just raw bytes.
        let atom = parse("a &custom; b", en(), Codec::verbatim()).unwrap();
        assert_nodes(&atom, &[(false, "a &custom; b")]);
    }

    #[test]
    fn logical_mode_rejects_unknown_entities() {
        let error = parse("a &custom; b", en(), Codec::logical()).unwrap_err();
        assert_eq!(
            error,
            ParseError::UnknownEntity {
                entity: "custom".to_owned()
            }
        );
    }

    #[test]
    fn non_inline_element_is_rejected() {
        let error = parse_logical("<span>x</span>").unwrap_err();
        assert_eq!(
            error,
            ParseError::UnexpectedElement {
                name: "span".to_owned()
            }
        );
    }

    #[test]
    fn comment_is_unsupported() {
        let error = parse_logical("a<!-- note -->b").unwrap_err();
        assert_eq!(
            error,
            ParseError::UnsupportedConstruct {
                construct: "comment".to_owned()
            }
        );
    }

    #[test]
    fn mismatched_end_tag_is_rejected() {
        // Genuinely ill-formed XML (an end tag that doesn't match its open) is
        // caught by the XML layer and surfaces as Malformed.
        assert!(matches!(
            parse_logical(r#"<g id="1">x</b>"#),
            Err(ParseError::Malformed { .. })
        ));
    }

    #[test]
    fn unclosed_container_is_recorded_faithfully() {
        // We do not validate structural balance (D26 — no validation). An
        // unclosed <g> is recorded as exactly what it is and round-trips; whether
        // that is "valid" is a layer-above concern.
        let atom = parse_logical(r#"<g id="1">oops"#).unwrap();
        assert_nodes(&atom, &[(true, r#"<g id="1">"#), (false, "oops")]);
    }

    #[test]
    fn outer_whitespace_is_preserved_as_text() {
        // Trimming is identity's job, not the adapter's: the run is faithful.
        let atom = parse_logical("  hi  ").unwrap();
        assert_nodes(&atom, &[(false, "  hi  ")]);
    }
}
