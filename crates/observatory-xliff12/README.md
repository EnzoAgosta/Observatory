# observatory-xliff12

The XLIFF 1.2 adapter for [`observatory-core`]: a stateless codec between an
XLIFF 1.2 content fragment and the `ContentNode`s an `Atom` is made of.

It does exactly two things:

- **`parse_segment`** turns the XML of a content fragment — the inline body of a
  `<source>` or `<target>` — into a `Vec<ContentNode>`.
- **`emit_segment`** turns those nodes back into that XML.

That is the whole crate. It builds no document model, validates no document,
walks no `<file>` / `<trans-unit>` structure, and records no relationships
between strings. Assembling the `Atom` (with the language the caller tracks — it
lives a level up, on the `<file>` element), deciding _which_ fragment becomes an
atom, normalizing it, and how atoms relate are all the caller's job; this crate
is the boundary primitive those higher layers rest on.

## Scope

The adapter targets **XLIFF 1.2 only**. It does not handle XLIFF 2.0 / 2.1 / 2.2,
whose different inline model (`pc` / `sc` / `ec`) the market never widely adopted;
committing to 1.2 makes the adapter more precise, not less. Vendor dialects
(memoQ `.mqxliff`, SDL `.sdlxliff`) are XLIFF 1.2 underneath and may be added as
dialects later.

## How inline elements are recorded

A fragment is tokenized into the two kinds of content node an `Atom` is made of —
**text** and opaque **placeholder** — by a single rule: what the XLIFF 1.2 spec
says an element's content is.

| XLIFF 1.2 element             | Spec content      | Recorded as                                         |
| ----------------------------- | ----------------- | --------------------------------------------------- |
| `<bpt>` `<ept>` `<ph>` `<it>` | native code       | the whole element → one opaque placeholder          |
| `<g>` `<mrk>`                 | translatable text | open and close tags → placeholders; inner text kept |
| `<x/>` `<bx/>` `<ex/>`        | empty             | one placeholder                                     |
| (character data)              | text              | a text node                                         |

In short: if the spec says an element's content is _code_, the whole element is
hidden away as one placeholder; if it says _text_, the tags are recorded as
placeholders and the text between them is kept. A placeholder stores the element's
**raw markup**, exactly as it appeared (attribute order and quoting included), and
that markup is never interpreted — so a `<sub>` nested inside native code is
carried along opaquely rather than pulled out. The adapter records structure
faithfully and leaves identity to the core, which distinguishes placeholders only
by their position and count, never by their markup.

## Entity handling

How XML entities in text are treated is the `EntityMode`, and it must be the
same on both sides of a round-trip:

- **Logical** (the default): text is unescaped to its Unicode form on parse and
  re-escaped on emit. A round-trip is _content-identical_ — fragments that differ
  only in how a character was escaped (`&amp;`, `&#38;`, `&#x26;`) collapse to the
  same atom, which is what lets the same content produce the same identity
  everywhere.
- **Verbatim**: text is kept exactly as written, entities and all. A round-trip is
  _byte-identical_, at the cost of identity then depending on the original
  escaping.

CDATA sections are character data and follow the same mode — kept raw under
verbatim, their delimiters stripped under logical. Either way, placeholder markup
is always carried verbatim.

## Example

```rust
use observatory_xliff12::{EntityMode, emit::emit_segment, parse::parse_segment};

// Parse the inline body of a <source>/<target>. <g> wraps translatable text,
// so "here" is kept while the tags become opaque placeholders.
let nodes = parse_segment(r#"Click <g id="1">here</g>"#, EntityMode::Logical).unwrap();

// Emitting under the same mode reproduces the original content.
assert_eq!(
    emit_segment(&nodes, EntityMode::Logical),
    r#"Click <g id="1">here</g>"#,
);

// The caller then assembles the Atom with the language they track —
// Atom::new(language, nodes) — normalizes it, and takes its id.
```

## Relationship to observatory-core

The adapter is a thin boundary: it depends on `observatory-core` for the atom
types and `AtomId`, and keeps the messy outside world (XML parsing, dialect
quirks) at arm's length so the core stays format-free. Round-trip fidelity is the
property that keeps the two honest.

## Not handled here

These are deliberately left to the caller or to later work, not silently done:

- **Language-only tags.** XLIFF 1.2 often carries a language with no region
  (`source-language="en"`), but a `LanguageTag` requires one. Resolving a missing
  region is the caller's call before handing content to `parse_segment`.
- **`<sub>` extraction.** Translatable text nested inside native code is preserved
  opaquely; pulling it out into its own atom is a higher-layer decision.
- **`<mrk>` semantics.** Annotation markers are currently recorded like `<g>`
  (tags marked, text kept); whether they should affect identity is still open.
- **Whole-document concerns** — file structure, validation, the source↔target
  relationship — live above this crate.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.

[`observatory-core`]: ../observatory-core/
