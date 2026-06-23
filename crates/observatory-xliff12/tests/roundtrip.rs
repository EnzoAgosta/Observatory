//! The round-trip fidelity gate (D9): parsing an XLIFF 1.2 content fragment and
//! emitting it back must be faithful.
//!
//! - **Verbatim** is byte-identical for any fragment.
//! - **Logical** is byte-identical when the input uses canonical escaping
//!   (`&amp;`, `&lt;`, `&gt;`), and otherwise *content-identical* — equivalent
//!   escapings collapse to one canonical form, which is exactly what makes the
//!   `AtomId` reproducible across encodings.

use observatory_core::identity::atom_id;
use observatory_core::ir::LanguageTag;
use observatory_core::normalize::NormalizationProfile;
use observatory_xliff12::{Codec, emit, parse};

fn en() -> LanguageTag {
    LanguageTag::parse("en-us").unwrap()
}

/// Fragments that use canonical escaping and no literal `<`, `>`, `&`, so they
/// round-trip byte-identically under *both* codecs.
const CANONICAL_FRAGMENTS: &[&str] = &[
    "",
    "Hello, world",
    "  leading and trailing  ",
    r#"Click <g id="1">here</g>"#,
    r#"<g id="1">a<g id="2">b</g>c</g>"#,
    r#"a<ph id="1">&lt;br/&gt;</ph>b"#,
    r#"<bpt id="1">&lt;b&gt;</bpt>bold<ept id="1">&lt;/b&gt;</ept>"#,
    r#"start<x id="1"/>mid<bx id="2"/>in<ex id="2"/>end"#,
    r#"<ph id="1">img(<sub>alt text</sub>)</ph>"#,
    r#"<mrk mtype="term">CPU</mrk> usage"#,
    "Tom &amp; Jerry &lt;3 &gt; 2",
];

#[test]
fn verbatim_round_trip_is_byte_identical() {
    for fragment in CANONICAL_FRAGMENTS {
        let atom = parse(fragment, en(), Codec::verbatim()).unwrap();
        assert_eq!(
            emit(&atom, Codec::verbatim()),
            *fragment,
            "verbatim: {fragment:?}"
        );
    }
}

#[test]
fn verbatim_round_trip_preserves_noncanonical_escaping() {
    // Verbatim keeps bytes exactly, including numeric refs and custom entities.
    for fragment in [
        "A&#38;B&#x3c;C",
        "a &custom; b",
        r#"keep "quotes" and 'apostrophes'"#,
    ] {
        let atom = parse(fragment, en(), Codec::verbatim()).unwrap();
        assert_eq!(emit(&atom, Codec::verbatim()), fragment);
    }
}

#[test]
fn logical_round_trip_is_byte_identical_for_canonical_escaping() {
    for fragment in CANONICAL_FRAGMENTS {
        let atom = parse(fragment, en(), Codec::logical()).unwrap();
        assert_eq!(
            emit(&atom, Codec::logical()),
            *fragment,
            "logical: {fragment:?}"
        );
    }
}

#[test]
fn logical_round_trip_normalizes_escaping_to_canonical() {
    // Numeric and entity forms all decode to the same characters and re-emit in
    // one canonical escaped form — content-identical, not byte-identical.
    let atom = parse("A&#38;B&#x3c;C", en(), Codec::logical()).unwrap();
    assert_eq!(emit(&atom, Codec::logical()), "A&amp;B&lt;C");
}

#[test]
fn logical_emit_is_idempotent() {
    // Re-parsing an emitted fragment and emitting again is a fixed point.
    for fragment in CANONICAL_FRAGMENTS {
        let once = emit(
            &parse(fragment, en(), Codec::logical()).unwrap(),
            Codec::logical(),
        );
        let twice = emit(
            &parse(&once, en(), Codec::logical()).unwrap(),
            Codec::logical(),
        );
        assert_eq!(once, twice, "idempotence: {fragment:?}");
    }
}

fn id(fragment: &str, codec: Codec) -> observatory_core::identity::AtomId {
    atom_id(
        &parse(fragment, en(), codec).unwrap(),
        &NormalizationProfile::default(),
    )
}

#[test]
fn equivalent_escapings_share_atom_id_under_logical() {
    // The §10 reproducibility bet at the boundary: how a character was escaped
    // upstream must not fragment identity.
    let amp = id("Tom &amp; Jerry", Codec::logical());
    let dec = id("Tom &#38; Jerry", Codec::logical());
    let hex = id("Tom &#x26; Jerry", Codec::logical());
    assert_eq!(amp, dec);
    assert_eq!(amp, hex);
}

#[test]
fn placeholder_markup_does_not_affect_atom_id() {
    // Same text and same placeholder positions, different attributes → same id.
    let plain = id(r#"Click <g id="1">here</g>"#, Codec::logical());
    let decorated = id(r#"Click <g id="9" ctype="bold">here</g>"#, Codec::logical());
    assert_eq!(plain, decorated);
}

#[test]
fn different_tag_kinds_with_matching_structure_share_atom_id() {
    // A paired <g> and a pair of empty placeholders both yield one placeholder
    // before and after "here" — identical position and count, so identical id
    // (D16: identity is by position and count, never by kind).
    let paired = id(r#"Click <g id="1">here</g>"#, Codec::logical());
    let empties = id(r#"Click <bx id="1"/>here<ex id="1"/>"#, Codec::logical());
    assert_eq!(paired, empties);
}

#[test]
fn logical_and_verbatim_can_diverge_on_atom_id() {
    // Verbatim hashes the escaped bytes; logical hashes the decoded text. For an
    // escaped fragment the two deliberately differ.
    assert_ne!(
        id("Tom &amp; Jerry", Codec::logical()),
        id("Tom &amp; Jerry", Codec::verbatim()),
    );
}
