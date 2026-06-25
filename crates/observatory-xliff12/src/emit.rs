//! Emitting [`ContentNode`]s back to an XLIFF 1.2 content fragment.
//!
//! [`emit_segment`] is the inverse of
//! [`parse_segment`](crate::parse::parse_segment), under the same [`EntityMode`].

use observatory_core::ir::ContentNode;
use quick_xml::escape::partial_escape;

use crate::parse::EntityMode;

/// Serializes content nodes into an XLIFF 1.2 inline fragment — the inverse of
/// [`parse_segment`](crate::parse::parse_segment) under the same `mode`.
///
/// Placeholders are always written raw (they already hold markup). Under
/// [`EntityMode::Verbatim`] text is written raw too, so the result is
/// byte-identical to what was parsed; under [`EntityMode::Logical`] text is
/// re-escaped (`<`, `>`, `&`; quotes left untouched), so the result is
/// content-identical.
///
/// The escape set is deliberately narrower than
/// [`parse_segment`](crate::parse::parse_segment)'s decoding: parse decodes *all*
/// standard entities (so `&quot;` and `"` share an identity), while emit escapes
/// only what XML text content requires and leaves quotes raw. The two compose to
/// content-identity, not byte-identity.
pub fn emit_segment(content: &[ContentNode], mode: EntityMode) -> String {
    match mode {
        EntityMode::Verbatim => content.iter().map(ContentNode::as_str).collect(),
        EntityMode::Logical => content
            .iter()
            .map(|node| match node {
                ContentNode::Text(_) => partial_escape(node.as_str()).into_owned(),
                ContentNode::Placeholder(_) => node.as_str().to_owned(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_segment;

    fn text(data: &str) -> ContentNode {
        ContentNode::text(data)
    }

    fn placeholder(data: &str) -> ContentNode {
        ContentNode::placeholder(data)
    }

    #[test]
    fn verbatim_joins_nodes_raw() {
        let nodes = [text("Tom &amp; "), placeholder("<x/>"), text("Jerry")];
        assert_eq!(
            emit_segment(&nodes, EntityMode::Verbatim),
            "Tom &amp; <x/>Jerry"
        );
    }

    #[test]
    fn logical_escapes_markup_characters_in_text() {
        assert_eq!(
            emit_segment(&[text("a < b & c > d")], EntityMode::Logical),
            "a &lt; b &amp; c &gt; d"
        );
    }

    #[test]
    fn logical_leaves_quotes_unescaped() {
        // partial escaping, not full — quotes are legal raw in text content.
        assert_eq!(
            emit_segment(&[text(r#"say "hi" it's fine"#)], EntityMode::Logical),
            r#"say "hi" it's fine"#
        );
    }

    #[test]
    fn placeholders_are_emitted_raw_in_both_modes() {
        let nodes = [placeholder(r#"<ph id="1">&amp;</ph>"#)];
        assert_eq!(
            emit_segment(&nodes, EntityMode::Logical),
            r#"<ph id="1">&amp;</ph>"#
        );
        assert_eq!(
            emit_segment(&nodes, EntityMode::Verbatim),
            r#"<ph id="1">&amp;</ph>"#
        );
    }

    #[test]
    fn verbatim_round_trip_is_byte_identical() {
        for input in [
            "plain text",
            r#"a<g id="1">b &amp; c</g><x/>d"#,
            r#"<ph id="1">x</ph>"#,
        ] {
            let nodes = parse_segment(input, EntityMode::Verbatim).unwrap();
            assert_eq!(emit_segment(&nodes, EntityMode::Verbatim), input);
        }
    }

    #[test]
    fn logical_round_trip_preserves_content() {
        // Logical is content-identical, not byte-identical: re-parsing the emitted
        // fragment yields the same nodes (e.g. CDATA is re-serialized as escaped
        // text, but the content is unchanged).
        for input in [
            "Tom &amp; Jerry &lt;3",
            r#"a<g id="1">x</g>b"#,
            "a<![CDATA[b & c]]>d",
        ] {
            let nodes = parse_segment(input, EntityMode::Logical).unwrap();
            let emitted = emit_segment(&nodes, EntityMode::Logical);
            assert_eq!(parse_segment(&emitted, EntityMode::Logical).unwrap(), nodes);
        }
    }
}
