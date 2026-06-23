# observatory-xliff12

The XLIFF 1.2 adapter for [`observatory-core`]: a stateless codec between an
XLIFF 1.2 content fragment and an `Atom`.

It does exactly two things:

- **`parse`** turns the XML of a content fragment — the inline body of a
  `<source>` or `<target>` — into an `Atom`.
- **`emit`** turns an `Atom` back into that XML.

That is the whole crate. It builds no document model, validates no document,
walks no `<file>` / `<trans-unit>` structure, and records no relationships
between strings. Deciding *which* fragment becomes an atom, and how atoms relate,
is the caller's job — this crate is the boundary primitive those higher layers
rest on. The caller also supplies the language (it lives a level up, on the
`<file>` element), which doubles as a check that their input is what they think.

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

| XLIFF 1.2 element                     | Spec content      | Recorded as                                              |
| ------------------------------------- | ----------------- | ------------------------------------------------------- |
| `<bpt>` `<ept>` `<ph>` `<it>`         | native code       | the whole element → one opaque placeholder              |
| `<g>` `<mrk>`                         | translatable text | open and close tags → placeholders; inner text kept     |
| `<x/>` `<bx/>` `<ex/>`                | empty             | one placeholder                                          |
| (character data)                      | text              | a text node                                              |

In short: if the spec says an element's content is *code*, the whole element is
hidden away as one placeholder; if it says *text*, the tags are recorded as
placeholders and the text between them is kept. A placeholder stores the element's
**raw markup**, exactly as it appeared (attribute order and quoting included), and
that markup is never interpreted — so a `<sub>` nested inside native code is
carried along opaquely rather than pulled out. The adapter records structure
faithfully and leaves identity to the core, which distinguishes placeholders only
by their position and count, never by their markup.

## Entity handling

How XML entities in text are treated is the codec's one choice, and it must be the
same on both sides of a round-trip:

- **Logical** (the default): text is unescaped to its Unicode form on parse and
  re-escaped on emit. A round-trip is *content-identical* — fragments that differ
  only in how a character was escaped (`&amp;`, `&#38;`, `&#x26;`) collapse to the
  same atom, which is what lets the same content produce the same identity
  everywhere.
- **Verbatim**: text is kept exactly as written, entities and all. A round-trip is
  *byte-identical*, at the cost of identity then depending on the original
  escaping.

Either way, placeholder markup is always carried verbatim.

## Example

```rust
use observatory_core::ir::LanguageTag;
use observatory_xliff12::{Codec, emit, parse};

let language = LanguageTag::parse("en-US").unwrap();

// Parse the inline body of a <source>/<target>. <g> wraps translatable text,
// so "here" is kept while the tags become opaque placeholders.
let atom = parse(r#"Click <g id="1">here</g>"#, language, Codec::logical()).unwrap();

// Emitting reproduces the original content.
assert_eq!(emit(&atom, Codec::logical()), r#"Click <g id="1">here</g>"#);
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
  region is the caller's call before handing content to `parse`.
- **`<sub>` extraction.** Translatable text nested inside native code is preserved
  opaquely; pulling it out into its own atom is a higher-layer decision.
- **`<mrk>` semantics.** Annotation markers are currently recorded like `<g>`
  (tags marked, text kept); whether they should affect identity is still open.
- **Whole-document concerns** — file structure, validation, the source↔target
  relationship — live above this crate.

The reasoning behind these choices is recorded in the repository's
[`docs/DECISIONS.md`](../../docs/DECISIONS.md).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.

[`observatory-core`]: ../observatory-core/
