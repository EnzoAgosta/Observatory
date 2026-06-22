# Observatory — Decision Log

The reasoning record for Observatory, the normalization core of the translation
data layer. (Background: `../translation-data-layer-thesis.md`.)

This log is **append-only**, mirroring the system it documents. A decision is
never edited in place once **Accepted**; if we change our minds, we add a new
decision that **supersedes** the old one and update the old one's status to
`Superseded by Dn`. Open questions live in their own section and graduate into
decisions when resolved.

**Conventions**
- Decisions are numbered `D1, D2, …`, never renumbered or deleted.
- Open questions are numbered `Q1, Q2, …`. When resolved, mark `Resolved by Dn`.
- Statuses: `Accepted`, `Superseded by Dn`.

---

## Decision Log

### D1 — Scope: Observatory is the normalization engine, nothing else
**Status:** Accepted

**Context.** The wider system (per thesis) has two storage engines — Lance
(strings + embeddings, "what's near") and Kùzu (typed observations, "how things
relate") — plus retrieval, training, and an eventual business layer. The thesis
(§7, §11) identifies the normalization engine — defining the *atom* and proving
it round-trips — as the make-or-break artifact that gates everything downstream.

**Decision.** This project (`observatory`) implements *only* the normalization
core: the intermediate representation (IR), content-addressed identity, and the
XLIFF boundary. Lance, Kùzu, embeddings, retrieval, training, and orchestration
are explicitly out of scope here.

**Why.** "You can always derive the old TM from this model, but never the
reverse" only holds if the atom is defined crisply first. Build the foundation
before the floors that rest on it.

---

### D2 — Atom granularity: kind + pairing, no payload
**Status:** Accepted

**Context.** A segment with inline tags is "a string + a document-specific
structure" (§7). The same content with different tag encodings (`<g id=3>` vs
`<x/>`, bold vs link) must not fragment into different atoms.

**Decision.** The canonical content (the atom) keeps text plus **positional
placeholders that carry tag *kind* and *pairing* structure, but not payload**.
Structure is *content*; the original encoding is *occurrence data* (a future
observation), and never enters identity.

**Consequences.**
- `Click <a href=x>here</a>` and `Click <b>here</b>` → **same atom**
  (`Click ⟦open:1⟧here⟦close:1⟧`); they differ only in payload.
- `Click {0}here{1}` (two standalones) → **different atom**; the structure
  genuinely differs.

---

### D3 — Boundary: XLIFF only, and only version 1.2
**Status:** Accepted

**Context.** §8 argues the IR should be strict and narrow, with the messy outside
world held at an adapter boundary. The boundary format must be chosen.

**Decision.** XLIFF is the *one and only* boundary format. We target **XLIFF 1.2
specifically** and do not account for 2.0 / 2.1 / 2.2.

**Why.** Effectively every system in the field still emits 1.2. XLIFF 2.0's
inline model (`pc`/`sc`/`ec`) added complexity the market never adopted.
Committing to 1.2 makes the engine *more* precise and optimized, not less.
Vendor dialects (memoQ `.mqxliff`, SDL `.sdlxliff`) are XLIFF 1.2 underneath
(§8) and slot in as dialects later.

**Closed inline-tag set, mapped to XLIFF 1.2:**

| IR kind | XLIFF 1.2 elements |
|---|---|
| `Open` | `<g>` (start), `<bpt>`, `<bx/>`, `<it pos="open">` |
| `Close` | `<g>` (end), `<ept>`, `<ex/>`, `<it pos="close">` |
| `Standalone` | `<ph>`, `<x/>` |

Pairing is conveyed by 1.2's `id` / `rid` attributes, captured as our `link`.

---

### D4 — Identity hash: SHA-256
**Status:** Accepted

**Context.** The atom's primary key is a content hash. Candidates: SHA-1 (broken,
160-bit), SHA-256 (standard, 256-bit), BLAKE3 (modern, fast, Merkle-tree based),
plus SHA-512/SHA-3 and non-cryptographic hashes (xxHash/Murmur — disqualified, a
collision in a primary key silently merges two atoms).

**Decision.** **SHA-256.**

**Why.** Inputs are short segments, so throughput is irrelevant (XML parsing will
dominate, and BLAKE3's parallelism only helps on large inputs). The deciding
axis is that this ID is part of an **open standard meant to be adopted**:
SHA-256 is reproducible in any language with one line and zero exotic
dependencies (`sha256sum` exists everywhere), with no "what's BLAKE3?" friction.
Revisit only if hashing is ever *measured* to be a bottleneck.

---

### D5 — Identity composition: normalized content + canonical language tag, nothing else
**Status:** Accepted

**Decision.** `identity = SHA-256(normalized_unicode_content ‖ canonical_BCP-47_language_tag)`.
No other field is fed to the identity function.

**Guiding principle.** *Include in identity only what makes two byte-identical
strings genuinely different atoms. Everything that is a fact* about *an atom
rather than* constitutive of *it is an observation.*

- **Language** passes: `"Gift"` (en) and `"Gift"` (de) are different translatable
  objects sharing bytes; every downstream op (embedding, retrieval, dedup) is
  per-language.
- **Direction (source vs. target)** fails — this is the entire thesis; direction
  is the `TRANSLATION_OF` observation, never identity.
- **Domain, register, client, provenance, recency** all fail — textbook
  observations.
- **Tag payload** already excluded by D2.

Languages are represented in **BCP-47**, canonicalized for identity (case/script
normalization per the registry). The exact byte layout of `‖` (the
concatenation/serialization fed to SHA-256) is to be designed in Phase 1.

---

### D6 — Flexibility lives in normalization, not in the hash
**Status:** Accepted

**Context.** Considered making the hash itself pluggable so implementers could
choose how to differentiate strings.

**Decision.** The **identity formula is fixed** (D4 + D5). **Normalization is the
configurable surface**, exposed as *named profiles*: how Unicode content is
normalized (NF form, whitespace, case) and how BCP-47 tags are normalized (e.g.
whether `en-US` and `en-GB` fold together). We ship a canonical default profile.

**Why.** A content-addressed ID's value is that it is **globally reproducible** —
same string → same ID in every deployment, so graphs merge, corpora are
shareable, and there is a canonical public ID. Free-form hashing destroys that
(my IDs ≠ your IDs), is a foundational footgun, and is premature generalization
(§10's whole bet is to make this model *the standard*). Moving flexibility one
layer down to normalization preserves ~90% of the desired control while keeping
one reproducible hashing scheme. Same profile → shareable IDs for free;
divergence is a deliberate, *named* choice. If truly alternate identity schemes
are ever needed, add *named* schemes then — not on spec.

---

### D7 — Language tag: lowercase, region mandatory, no folding, external parser
**Status:** Accepted &nbsp;|&nbsp; **Resolves Q1**

**Decision.**
- BCP-47 tags fed to identity are normalized to **all lowercase**.
- A **region subtag is mandatory**. A tag without a region **fails loud and
  fast** (validation error) — we never guess or default one.
- **No region folding**: `en-us` ≠ `en-gb`, `nl-be` ≠ `nl-nl`. Region variants
  differ in orthography, terminology, and register; they are different atoms.
- BCP-47 **parsing/validation uses an external crate**, not a hand-rolled
  parser. (Exact crate chosen in Phase 1d.)

**Notes.**
- All-lowercase is *our* deterministic identity form, deliberately simpler than
  BCP-47's recommended case form (language lower / script Title / region UPPER).
  BCP-47 is case-insensitive, so any single deterministic casing is a valid
  identity key; lowercase is the simplest.
- Robust BCP-47 handling (grammar + IANA registry rules) is non-trivial; leaning
  on a vetted parser beats reimplementing it.

**Consequence → Q5.** XLIFF 1.2 frequently carries *language-only* tags
(`source-language="en"`). "Region mandatory" means such tags cannot form an
identity as-is; resolving the region becomes a layer-above responsibility.

---

### D8 — `<sub>` is a layer-above concern; identity stays oblivious
**Status:** Accepted &nbsp;|&nbsp; **Resolves the `<sub>` half of Q2**

**Decision.** The identity/IR layer does **not** special-case `<sub>`
(translatable text nested inside a code). The application/adapter layer extracts
sub-content as its **own string with its own identity and observations** — and
"this string was a sub-segment of X" is itself just another observation. The
identity function never needs to know `<sub>` exists.

**Why.** Consistent with the whole model: anything that looks like it wants to be
a special case *on* the atom is usually an observation a layer up. Extraction is
an adapter behavior (Phase 2+), not an identity concern.

---

### D9 — Testing is a first-class citizen
**Status:** Accepted

**Decision.** Every function we create is tested. We are **not** doing strict TDD
(tests need not come first), but no function ships untested. Test layers:
- **Unit tests** colocated per module (`#[cfg(test)] mod tests`).
- **Integration tests** in `tests/` for cross-module behavior (e.g. round-trip).
- **Property tests** for invariants (normalization idempotence, structural
  distinction, hash determinism).
- **Snapshot tests** for the canonical serialization and XLIFF round-trip
  fidelity.

(Property/snapshot tooling is a dependency choice — see the Phase 0 dependency
proposal.)

---

### D10 — Crate layout: single library crate for now
**Status:** Accepted

**Decision.** `observatory` is a **library crate** (no binary) with modules `ir`,
`identity`, `normalize` (and `xliff` in Phase 2). Promote to a Cargo **workspace**
(e.g. `observatory-core` + `observatory-xliff` + a CLI) only when a concrete need
justifies the split — not pre-emptively.

**Why.** It is the reusable open-core primitive (D1), so a library is the right
shape. KISS: one crate with clear module boundaries; split later if it earns it.

---

## Open Questions

### Q1 — Region folding default
**Resolved by D7.** No folding; region mandatory; all-lowercase.

### Q2 — `<mrk>` handling
The `<sub>` half is resolved (D8). `<mrk>` (annotation spans) is the harder case
and remains open: annotations can wrap arbitrary content and carry semantics
(terms, comments) that may or may not belong in identity. Deferred past Phase 1.

### Q3 — Canonical serialization byte layout
The precise, stable byte encoding of `(content placeholders + text) ‖ language`
fed to SHA-256. The crux of Phase 1 (sub-phase 1b) — must be unambiguous and
version-stable. Needs dedicated discussion before 1b.

### Q4 — Normalization profile interface
The shape of a "named profile" in Rust: trait, config struct, or data. To be
designed in Phase 1d, once the IR types exist.

### Q5 — Language-only tags at the XLIFF boundary
D7 makes region mandatory, but XLIFF 1.2 commonly uses language-only tags
(`source-language="en"`). Decide how the XLIFF adapter (Phase 2) handles a
missing region: reject outright, or require the caller to supply a
region-resolution policy. Out of scope until Phase 2; recorded so D7's
consequence isn't lost.

---

## Phase Plan (accepted)

Deliberately fine-grained; we reason through each before starting it.

- **Phase 0 — Foundations.** This decision log; crate scaffolding (D10), the
  testing harness (D9), lint/format setup, and the dependency proposal.
- **Phase 1 — IR + identity, XLIFF-1.2-informed but format-free.**
  - 1a. IR types (the data model; types only, no behavior).
  - 1b. Canonical serialization (the exact bytes fed to the hash — resolves Q3).
  - 1c. Identity / SHA-256 over that serialization.
  - 1d. Normalization profile interface + canonical default, using an external
    BCP-47 crate (resolves Q4; implements D7).
  - 1e. Invariant tests (same-content-diff-payload → same id; diff structure →
    diff id; `normalize(normalize(x)) == normalize(x)`).
- **Phase 2 — XLIFF 1.2 adapter + fidelity gate.** `parse: XLIFF 1.2 → IR`,
  `emit: IR → XLIFF 1.2`, round-trip diff test as a CI gate; language-only-tag
  policy (Q5); `<sub>` extraction (D8).
- **Phase 3+ — Dialects & hard cases.** memoQ / SDL dialects; `<mrk>` (Q2);
  normalization refinements.
