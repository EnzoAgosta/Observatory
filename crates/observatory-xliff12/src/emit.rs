//! Emitting an [`Atom`] back to XLIFF 1.2 content — the inverse of
//! [`parse`](crate::parse).
//!
//! Emission is a straight in-order walk of the atom's nodes: placeholders are
//! written as their raw stored markup, and text is written per the [`Codec`]. It
//! is infallible — every atom emits.

use crate::codec::{Codec, EntityMode};
use observatory_core::ir::Atom;
use quick_xml::escape::partial_escape;

/// Emits `atom` as an XLIFF 1.2 content fragment, treating text per `codec`.
///
/// Placeholder markup is written verbatim (it was stored raw). Under
/// [`EntityMode::Logical`] text is re-escaped with partial escaping (`<`, `>`,
/// `&`; quotes are left alone, since they need no escaping in element content);
/// under [`EntityMode::Verbatim`] text is written exactly as stored. Use the same
/// `codec` that produced the atom.
pub fn emit(atom: &Atom, codec: Codec) -> String {
    let mut out = String::new();
    for node in atom.content() {
        if node.is_placeholder() {
            out.push_str(node.data());
        } else {
            match codec.entities {
                EntityMode::Logical => out.push_str(&partial_escape(node.data())),
                EntityMode::Verbatim => out.push_str(node.data()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_core::ir::{ContentNode, LanguageTag};

    fn atom(nodes: impl IntoIterator<Item = ContentNode>) -> Atom {
        Atom::new(LanguageTag::parse("en-us").unwrap(), nodes)
    }

    #[test]
    fn emits_plain_text() {
        let atom = atom([ContentNode::text("Hello, world")]);
        assert_eq!(emit(&atom, Codec::logical()), "Hello, world");
    }

    #[test]
    fn writes_placeholder_markup_verbatim() {
        let atom = atom([
            ContentNode::text("Click "),
            ContentNode::placeholder(r#"<g id="1">"#),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ]);
        assert_eq!(emit(&atom, Codec::logical()), r#"Click <g id="1">here</g>"#);
    }

    #[test]
    fn logical_mode_escapes_text_specials() {
        let atom = atom([ContentNode::text("Tom & Jerry <3")]);
        assert_eq!(emit(&atom, Codec::logical()), "Tom &amp; Jerry &lt;3");
    }

    #[test]
    fn logical_mode_leaves_quotes_unescaped() {
        // Quotes need no escaping in element content; partial escaping leaves them.
        let atom = atom([ContentNode::text(r#"say "hi" it's fine"#)]);
        assert_eq!(emit(&atom, Codec::logical()), r#"say "hi" it's fine"#);
    }

    #[test]
    fn verbatim_mode_does_not_escape_text() {
        // Verbatim text is already in its escaped form; emit must not double-escape.
        let atom = atom([ContentNode::text("Tom &amp; Jerry")]);
        assert_eq!(emit(&atom, Codec::verbatim()), "Tom &amp; Jerry");
    }

    #[test]
    fn placeholder_markup_is_never_escaped() {
        // Even with text-affecting modes, raw markup passes through untouched.
        let atom = atom([ContentNode::placeholder(r#"<ph id="1">&lt;br/&gt;</ph>"#)]);
        assert_eq!(
            emit(&atom, Codec::logical()),
            r#"<ph id="1">&lt;br/&gt;</ph>"#
        );
    }

    #[test]
    fn empty_atom_emits_empty_string() {
        let atom = atom([]);
        assert_eq!(emit(&atom, Codec::logical()), "");
    }
}
