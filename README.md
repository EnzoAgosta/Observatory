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

## Storage shape

- **Lance** is the single storage substrate: atoms, embeddings, and the
  observation log all live as Lance tables. Columnar scans, random access, and
  built-in vector indexes cover the dominant translation workloads in one engine.
- **DuckDB** is a query engine over that data, not a store. It handles the 1–2
  hop joins and filtered scans that make up the common case (e.g. "all
  translations of this string approved by a human and not blacklisted, as of ≤3
  months ago").
- **The graph is a derived view**, not a store of record. A recursive graph engine
  is only revisited if a concrete query proves unserveable by Lance + DuckDB.

## Repository layout

This repo is a Cargo workspace of independently releasable crates:

- **[`observatory-core`](crates/observatory-core/)** — the normalization engine:
  the atom IR, content-addressed `AtomId`, and normalization. The foundation
  everything else rests on.
- **[`observatory-xliff12`](crates/observatory-xliff12/)** — the XLIFF 1.2 adapter:
  parse and emit atoms at the boundary with the outside world. (Phase 2; currently
  a skeleton.)

New here? [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md) lays out the mental model —
how to think about atoms, identity, and where responsibilities live — and is the
best starting point for a human or an AI. Decisions and their reasoning live in
[`docs/DECISIONS.md`](docs/DECISIONS.md), an append-only log mirroring the
observation model it documents.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
