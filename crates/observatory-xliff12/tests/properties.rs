//! Property-based invariants for the parse/emit round-trip, over randomly
//! generated well-formed XLIFF 1.2 content fragments.
//!
//! The generator (`arb_content`) builds valid inline content: text (with the XML
//! specials escaped), empty inline elements, code elements (optionally carrying a
//! `<sub>`), and recursively nested `<g>` / `<mrk>` containers. The invariants:
//!
//! - verbatim parse→emit reproduces the input byte-for-byte;
//! - logical parse→emit is a fixed point (idempotent);
//! - a logical round-trip preserves the atom's identity.

use observatory_core::identity::atom_id;
use observatory_core::ir::LanguageTag;
use observatory_core::normalize::NormalizationProfile;
use observatory_xliff12::{Codec, emit, parse};
use proptest::prelude::*;

fn en() -> LanguageTag {
    LanguageTag::parse("en-us").unwrap()
}

/// Escapes the three XML specials so generated text is well-formed inside an
/// element. Quotes are left literal — they need no escaping in element content.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A short string over a deliberately tricky alphabet: the XML specials, quotes,
/// whitespace (including a non-breaking space), and a few non-ASCII characters.
fn arb_raw_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('Z'),
            Just('0'),
            Just(' '),
            Just('\t'),
            Just('\n'),
            Just('<'),
            Just('>'),
            Just('&'),
            Just('"'),
            Just('\''),
            Just('é'),
            Just('漢'),
            Just('\u{00a0}'),
        ],
        0..6,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

/// Generated text in its XML-escaped form, ready to embed in a fragment.
fn arb_text() -> impl Strategy<Value = String> {
    arb_raw_text().prop_map(|text| xml_escape(&text))
}

/// A small attribute value (used for `id`, `mid`).
fn arb_id() -> impl Strategy<Value = String> {
    (0u32..5).prop_map(|n| n.to_string())
}

/// An empty inline element: `<x/>`, `<bx/>`, or `<ex/>`.
fn arb_empty_inline() -> impl Strategy<Value = String> {
    (prop_oneof![Just("x"), Just("bx"), Just("ex")], arb_id())
        .prop_map(|(name, id)| format!(r#"<{name} id="{id}"/>"#))
}

/// The content of a code element: escaped native code, sometimes wrapping a
/// translatable `<sub>` (which the parser must swallow opaquely).
fn arb_code_content() -> impl Strategy<Value = String> {
    prop_oneof![
        arb_text(),
        (arb_text(), arb_text()).prop_map(|(code, sub)| format!("{code}<sub>{sub}</sub>")),
    ]
}

/// A code element: `<ph>`, `<bpt>`, `<ept>`, or `<it>` (the latter carrying the
/// required `pos`), with native code content.
fn arb_code_element() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just("ph"), Just("bpt"), Just("ept"), Just("it")],
        arb_id(),
        arb_code_content(),
    )
        .prop_map(|(name, id, content)| {
            let attrs = if name == "it" {
                format!(r#" id="{id}" pos="open""#)
            } else {
                format!(r#" id="{id}""#)
            };
            format!("<{name}{attrs}>{content}</{name}>")
        })
}

/// A whole content fragment: a sequence of items, where an item is text, an empty
/// inline, a code element, or a `<g>` / `<mrk>` container nesting more items.
fn arb_content() -> impl Strategy<Value = String> {
    let item = prop_oneof![arb_text(), arb_empty_inline(), arb_code_element()];
    let item = item.prop_recursive(3, 24, 4, |inner| {
        (
            prop_oneof![Just("g"), Just("mrk")],
            arb_id(),
            prop::collection::vec(inner, 0..4),
        )
            .prop_map(|(name, id, children)| {
                let attrs = if name == "mrk" {
                    format!(r#" mtype="x" mid="{id}""#)
                } else {
                    format!(r#" id="{id}""#)
                };
                format!("<{name}{attrs}>{}</{name}>", children.concat())
            })
    });
    prop::collection::vec(item, 0..5).prop_map(|items| items.concat())
}

proptest! {
    /// Verbatim mode captures every byte, so emitting reproduces the input exactly.
    #[test]
    fn verbatim_round_trip_is_byte_identical(content in arb_content()) {
        let atom = parse(&content, en(), Codec::verbatim()).unwrap();
        prop_assert_eq!(emit(&atom, Codec::verbatim()), content);
    }

    /// Logical emission is a fixed point: parsing an emitted fragment and emitting
    /// it again yields the same string.
    #[test]
    fn logical_emit_is_idempotent(content in arb_content()) {
        let once = emit(&parse(&content, en(), Codec::logical()).unwrap(), Codec::logical());
        let twice = emit(&parse(&once, en(), Codec::logical()).unwrap(), Codec::logical());
        prop_assert_eq!(once, twice);
    }

    /// A logical round-trip never changes an atom's identity.
    #[test]
    fn logical_round_trip_preserves_atom_id(content in arb_content()) {
        let profile = NormalizationProfile::default();
        let atom = parse(&content, en(), Codec::logical()).unwrap();
        let reparsed = parse(&emit(&atom, Codec::logical()), en(), Codec::logical()).unwrap();
        prop_assert_eq!(atom_id(&atom, &profile), atom_id(&reparsed, &profile));
    }
}
