# observatory-xliff12

The XLIFF 1.2 adapter for [`observatory-core`]: parsing XLIFF 1.2 segments into
[`observatory_core::ir::Atom`]s and emitting atoms back to XLIFF 1.2.

## Scope

This adapter targets **XLIFF 1.2 only** (decision D3). It does not account for
XLIFF 2.0 / 2.1 / 2.2 — 2.0's inline model (`pc`/`sc`/`ec`) added complexity the
market never adopted, and committing to 1.2 makes the adapter more precise, not
less. Vendor dialects (memoQ `.mqxliff`, SDL `.sdlxliff`) are XLIFF 1.2 underneath
and slot in as dialects later (Phase 3+).

The closed inline-tag set maps XLIFF 1.2 elements onto the core's two content
node kinds (text / placeholder):

| Core kind   | XLIFF 1.2 elements                                                |
| ----------- | ----------------------------------------------------------------- |
| placeholder | `<g>`, `<bpt>`, `<ept>`, `<bx/>`, `<ex/>`, `<ph>`, `<x/>`, `<it>` |

A placeholder's raw markup is preserved for round-trip reconstruction but never
interpreted — identity distinguishes placeholders only by position and count
(D16), so the adapter records markup faithfully and leaves identity to the core.

## Relationship to observatory-core

The adapter is a thin boundary: it depends on `observatory-core` for the atom
types and `AtomId`, and holds the messy outside world (XML parsing, dialect
quirks) at arm's length so the core stays format-free. Round-trip fidelity
(parse → emit → byte-identical) is the CI gate that keeps the two in sync.

## Open scope

- **Language-only tags (Q5).** XLIFF 1.2 frequently carries language-only tags
  (`source-language="en"`), but the core requires a region (D7). The adapter must
  decide how to handle a missing region — reject outright, or accept a
  caller-supplied region-resolution policy. To be resolved when Phase 2 starts.
- **`<sub>` extraction (D8)** and **`<mrk>` annotations (Q2)** are layer-above
  concerns, deferred.

## Status

Phase 2 skeleton — parse/emit and the round-trip fidelity gate land here once
Phase 1 is settled.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.
