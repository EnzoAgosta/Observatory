//! Property-based round-trip fidelity for the XLIFF 1.2 adapter (D26, D9).
//!
//! A generator builds *valid* inline fragments — text, standard entities, and the
//! inline elements, nested — and renders them to a string. We then assert the two
//! round-trip guarantees: verbatim is byte-identical (`emit(parse(x)) == x`);
//! logical is content-identical (re-parsing the emitted form yields the same
//! nodes), since logical canonicalizes escaping and so can't be byte-identical.

use observatory_xliff12::{EntityMode, emit::emit_segment, parse::parse_segment};
use proptest::prelude::*;

/// One node of a generated inline fragment.
#[derive(Debug, Clone)]
enum Frag {
    /// A run of text that needs no escaping.
    Text(String),
    /// A standard entity or numeric character reference, e.g. `&amp;`.
    Entity(&'static str),
    /// A code-content element `<tag>inner</tag>`, captured opaquely.
    Code(&'static str, String),
    /// An empty inline element `<tag/>`.
    Empty(&'static str),
    /// A text-content element `<tag>children</tag>` (recursive).
    Group(&'static str, Vec<Frag>),
}

/// Renders a fragment to its XLIFF 1.2 string form.
fn render(frags: &[Frag]) -> String {
    let mut out = String::new();
    for frag in frags {
        match frag {
            Frag::Text(s) => out.push_str(s),
            Frag::Entity(e) => out.push_str(e),
            Frag::Code(tag, inner) => out.push_str(&format!("<{tag}>{inner}</{tag}>")),
            Frag::Empty(tag) => out.push_str(&format!("<{tag}/>")),
            Frag::Group(tag, children) => {
                out.push_str(&format!("<{tag}>"));
                out.push_str(&render(children));
                out.push_str(&format!("</{tag}>"));
            }
        }
    }
    out
}

/// Text needing no escaping (no `<`, `&`, or `>`).
fn arb_text() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Z0-9 .,!?]{0,6}").expect("valid regex")
}

/// A standard XML entity or numeric character reference.
fn arb_entity() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("&amp;"),
        Just("&lt;"),
        Just("&gt;"),
        Just("&quot;"),
        Just("&apos;"),
        Just("&#65;"),
        Just("&#x41;"),
    ]
}

/// A single fragment node, with text-content elements nesting recursively.
fn arb_frag() -> impl Strategy<Value = Frag> {
    let leaf = prop_oneof![
        arb_text().prop_map(Frag::Text),
        arb_entity().prop_map(Frag::Entity),
        (
            prop_oneof![Just("ph"), Just("bpt"), Just("ept"), Just("it")],
            arb_text(),
        )
            .prop_map(|(tag, inner)| Frag::Code(tag, inner)),
        prop_oneof![Just("x"), Just("bx"), Just("ex")].prop_map(Frag::Empty),
    ];
    leaf.prop_recursive(3, 24, 3, |inner| {
        (
            prop_oneof![Just("g"), Just("mrk")],
            prop::collection::vec(inner, 0..3),
        )
            .prop_map(|(tag, children)| Frag::Group(tag, children))
    })
}

fn arb_fragment() -> impl Strategy<Value = Vec<Frag>> {
    prop::collection::vec(arb_frag(), 0..6)
}

proptest! {
    /// Verbatim round-trips byte-for-byte: `emit(parse(x)) == x`.
    #[test]
    fn verbatim_round_trip_is_byte_identical(frags in arb_fragment()) {
        let fragment = render(&frags);
        let nodes = parse_segment(&fragment, EntityMode::Verbatim).unwrap();
        prop_assert_eq!(emit_segment(&nodes, EntityMode::Verbatim), fragment);
    }

    /// Logical round-trips at the content level: re-parsing the emitted form
    /// yields the same nodes (escaping is canonicalized, so it is not byte-exact).
    #[test]
    fn logical_round_trip_is_content_identical(frags in arb_fragment()) {
        let fragment = render(&frags);
        let nodes = parse_segment(&fragment, EntityMode::Logical).unwrap();
        let emitted = emit_segment(&nodes, EntityMode::Logical);
        let reparsed = parse_segment(&emitted, EntityMode::Logical).unwrap();
        prop_assert_eq!(reparsed, nodes);
    }
}
