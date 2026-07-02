# Observatory

A lakehouse for multilingual data.

Observatory turns translation segments into content-addressed **atoms** and
records facts about them as append-only **observations**. A traditional
translation memory encodes a single fact — "this source maps to this target" —
in a row and tacks context on as extra columns. Observatory decouples the strings
from the facts: an atom records only *what a string is*, and every fact about it
— `TRANSLATION_OF`, `APPROVED_BY`, `USED_IN_CAMPAIGN`, `BLACKLISTED`,
`CONTEXT_FOR`, `ALTERNATIVE_FOR` — is a separate observation over atoms. The old
TM is a derived projection of this richer structure; the structure is never a
derivative of the TM.

This is, in effect, event sourcing applied to translation: the observation log is
the source of truth, and everything else (the TM view, review state, reachability
queries) is a read model built on top. "Derive the old TM from this model, but
never the reverse" is the bet.

## Repository layout

This repo is a Cargo workspace of independently releasable crates:

- **[`observatory-core`](crates/observatory-core/)** — the normalization core: the
  atom IR, content-addressed `AtomId`, and normalization primitives. The
  foundation everything else rests on. *Implemented.*
- **[`observatory-observations`](crates/observatory-observations/)** — the
  append-only observation model: properties and relationships over atoms,
  bitemporal, with an open user-defined `Kind` and a JSON payload. *Implemented.*
- **[`observatory-xliff12`](crates/observatory-xliff12/)** — the XLIFF 1.2 boundary
  codec: parse and emit the content nodes of a `<source>`/`<target>` fragment,
  with configurable entity handling. *Implemented* (segment-level; whole-document
  structure is deliberately out of scope).
- **[`observatory-store`](crates/observatory-store/)** — Lance-backed persistence.
  Atom storage — create/open, dedup-on-write upsert, lookup by id — and
  observation storage — content-addressed writes, lookup by id and by kind — are
  implemented and tested against real on-disk datasets; `observations_about`,
  scalar indexes, and the query path are next. See its
  [`DESIGN.md`](crates/observatory-store/DESIGN.md). *In progress.*

## Storage architecture

The intended shape, and where it stands:

- **Lance** is the storage substrate. Atoms and observations are persisted today
  as content-addressed Lance datasets that dedup byte-identical writes; Lance's
  built-in vector indexes are the intended home for embeddings later. A Lance
  dataset is an immutable, versioned directory — every write is a new version,
  like a git commit.
- **DuckDB** *(planned, not yet wired up)* will be a query engine over that Lance
  data, not a second store — for the 1–2 hop joins and filtered scans that make up
  the common case (e.g. "all translations of this string approved by a human and
  not blacklisted, as of ≤3 months ago").
- **The graph is a derived view** *(aspirational)*, never a store of record. A
  recursive graph engine is revisited only if a concrete query proves unserveable
  by Lance + DuckDB.

## Status

Implemented today: the domain model end to end — atoms, identity, and
normalization (`observatory-core`), the observation model with content-addressed
identity (`observatory-observations`) — the XLIFF 1.2 segment codec
(`observatory-xliff12`), and **atom and observation persistence** on Lance
(`observatory-store`): write-with-dedup, point/equality lookups, the
`observations_about` array-membership query, scalar indexes (BTREE on
`atom_id`/`observation_id`, BITMAP on `kind`, LABEL_LIST on `subjects`), and
Lance maintenance primitives (`ensure_indexes`, `optimize_indexes`, `compact`,
`cleanup_versions`), all tested against real on-disk datasets.

Next, roughly in order: the DuckDB query path over the Lance datasets (compound
predicates, range queries, joins), then embeddings and vector search, further
format adapters (XLIFF dialects, TMX), and any graph view.

New here? [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md) lays out the mental model —
how to think about atoms, identity, and where responsibilities live — and is the
best starting point for a human or an AI.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
