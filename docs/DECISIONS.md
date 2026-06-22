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
**Status:** Superseded by D16

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

### D11 — BCP-47: enforce well-formedness, not registry validity; crate `oxilangtag`
**Status:** Accepted &nbsp;|&nbsp; **Refines D7**

**Decision.** The identity layer requires a language tag that is **well-formed**
(matches the BCP-47 grammar) **with a region subtag present** (D7), and
canonicalizes it to lowercase. It does **not** check **validity** (whether each
subtag exists in the IANA registry). Parsing uses **`oxilangtag`** (Oxigraph
project): well-formedness only, with subtag accessors, no bundled registry.

**Why.**
- *Validity is policy, and policy lives above the fixed identity layer (D6).*
  "Which tags count as real" is a caller/profile decision, not an identity one.
- *A legitimate use case needs it:* someone may deliberately use a synthetic or
  private locale (e.g. `qaa-zz`, or an `x-…` private-use tag) to force separate
  storage / control dedup. A library that rejected such tags would impose policy
  it has no business imposing.
- *The clean, predictable line is "well-formed + region present," not "registry
  valid."* Note BCP-47 already reserves registry-valid private-use ranges
  (`qaa`–`qtz`; regions `ZZ`, `QM`–`QZ`, `XA`–`XZ`; the `x-…` subtag), so even
  strict validation wouldn't behave as one might naively expect.

If registry validation is ever wanted, it becomes an optional normalization-
profile or app-layer check (D6 territory) — never baked into identity.

---

### D12 — License: dual `MIT OR Apache-2.0`
**Status:** Accepted

**Decision.** License the crate **`MIT OR Apache-2.0`** (the Rust-ecosystem
default), shipping both `LICENSE-MIT` and `LICENSE-APACHE` at the repo root and
declaring the SPDX expression in `Cargo.toml`.

**Why.** Permissive, matching §10's open-core intent. Dual-licensing costs
essentially nothing (two text files) while letting downstream users pick MIT's
simplicity or Apache-2.0's explicit patent grant + retaliation clause, and
maximizes compatibility (e.g. GPLv2 projects can consume the MIT side). Copyright
holder recorded as "Enzo Agosta, 2026" — adjust if a different attribution is
wanted.

---

### D13 — The canonical unit is named `Atom` (not `Segment`)
**Status:** Accepted

**Decision.** The content-addressed unit is the **`Atom`**; its content hash is
the **`AtomId`**.

**Why.** "Segment" is ubiquitous in the translation industry and carries
conventional baggage — it implies a source→target row in a TM. This project
deliberately *decouples* strings from relationships (D1), so naming the unit
`Atom` reinforces that it is a single, relation-free, content-addressed string,
not a TM segment.

---

### D14 — Identity is a dumb, reversible recording: positional renumbering, permissive structure
**Status:** Accepted

**Decision.**
- Building an `Atom` is a **dumb, reversible recording** of "what is text and
  what is tag," in order. The IR does **not** validate the input. Assumption:
  any XLIFF segment fed to us was *already valid* upstream.
- Tags are **always renumbered to appearance (positional) order**, regardless of
  the source's original ids — even if those ids were "correct." Original
  dialect ids are preserved in `payload` for round-trip.
- **Permissive on structure**: we do not enforce well-nested pairing (XLIFF 1.2
  allows overlap via `rid`). We require only canonical positional numbering;
  whatever pairing the input expresses, we record it and can reverse it.

**Why.** Round-trip fidelity demands we accept whatever valid XLIFF contains;
enforcing well-nesting would risk lossy round-trips and is policy that belongs
above identity (cf. D6). Canonical positional numbering is precisely what makes
the atom stable across differing dialect id schemes (D2).

---

### D15 — `Payload` stays opaque for Phase 1
**Status:** Superseded by D16

**Decision.** `Payload` — the recoverable, dialect-specific tag data that is
never hashed (D2) — is modeled as a thin **opaque blob** for Phase 1
(format-free). Its internal structure is deferred to Phase 2, when XLIFF 1.2
round-trip requirements are concrete.

**Why.** It never affects identity, so its shape should follow the round-trip
needs we will only know once we parse real XLIFF. Avoids speculative structure
(KISS).

---

### D16 — Uniform placeholders: identity by position and count, not kind
**Status:** Accepted &nbsp;|&nbsp; **Supersedes D2 and D15; refines D14**

**Context.** D2 had the atom keep placeholder *kind* (open / close / standalone)
and pairing in identity. But encoding kind requires *interpreting* each
placeholder's role — exactly the interpretation D14 says this layer must not do
— and the thesis (§7) lists a paired `<g>` vs a standalone `<ph/>` as "the same
content, different encodings" that should *not* fragment.

**Decision.**
- A content node is one of exactly **two** kinds: **Text** or **Placeholder**.
  No open / close / standalone distinction is recorded.
- A placeholder is **opaque**: its raw original markup is stored as the node's
  `data` and is never interpreted. (This folds D15 — there is no separate
  `Payload` type; the placeholder's `data` *is* the opaque payload.)
- **Identity distinguishes placeholders only by position and count**, never by
  kind or content. The placeholder's `data` never enters the hash (D2's
  no-payload-in-identity rule survives); each placeholder contributes a single
  uniform canonical marker to the hashed projection.
- **Reconstruction is the in-order join of every node's `data`** — a blind,
  lossless concatenation (the reversible half of D14).

**Consequence (accepted consciously).** Structurally different taggings of
identical text collapse to one atom: `Click <b>here</b>` (a pair) and
`Click {0}here{1}` (two standalones) share an `AtomId`. The translatable text is
identical; the tagging difference is occurrence data recoverable from `data`,
and "what kind of placeholder" — if ever wanted — is a derivation one layer up,
never part of identity. Round-trip is unaffected (it uses raw `data`, not
identity).

**Model.**
- `Atom { language, content: Vec<ContentNode> }` — order is significant.
- `ContentNode { kind: ContentKind, data: String }` — `data` is raw, UTF-8
  (XLIFF is XML text, D3).
- `ContentKind { Text, Placeholder }`.
- Canonical-by-construction reduces to: **merge adjacent Text, drop empty Text**
  (no links to renumber anymore).

---

### D17 — Faithful construction; structural normalization lives in the `AtomId`
**Status:** Accepted &nbsp;|&nbsp; **Refines D14; supersedes the "canonical-by-construction" clause and model sketch of D16**

**Context.** D16 had the `Atom` be *canonical by construction* — `Atom::new`
merged adjacent text and dropped empty runs. That makes construction silently
mutate the recording.

**Decision.**
- **Construction is a faithful, non-normalizing recording.** `Atom::new` stores
  exactly the nodes it is given. `[Text("a"), Text("b")]` stays distinct from
  `[Text("ab")]`; empty and adjacent runs are preserved. Construction carries
  zero logic.
- **Structural normalization — merge adjacent text, drop empty text — moves into
  the `AtomId` computation** (the hashing projection, Phase 1b–1c) as an
  explicit, independently-tested function. So `[Text("a"), Text("b")]` and
  `[Text("ab")]` are **different `Atom`s** that produce the **same `AtomId`**.
- This keeps the no-fragmentation guarantee (normalization still happens before
  hashing) while keeping the recording pure.

**Consequence (accepted; documented on the types).** Structural equality (`==`)
is a *different relation* from identity equality. Two `Atom`s may share an
`AtomId` without being `==`. **Dedup and identity are always via `AtomId`, never
`==`**; `==` means "structurally identical recording."

**Why.** A pure, surprise-free constructor is the strongest round-trip-safe
contract and the truest expression of "dumb recording" (D14); identity stays a
deliberate, derived projection — the `AtomId` is a *materialized view* over the
raw `Atom`, consistent with the thesis's raw-data-plus-views model.

**Model (updated; supersedes D16's sketch).**
- `Atom { language, content: Vec<ContentNode> }` — order significant; faithful,
  non-normalizing.
- `ContentNode { is_placeholder: bool, data: String }` — the closed binary is a
  `bool`, not an enum (`ContentKind` dropped). Constructors `text()` /
  `placeholder()`; accessor `is_placeholder()`.
- Reconstruction = in-order join of `data`. Normalization for hashing is a
  separate Phase 1b function.

---

### D18 — Canonical serialization scheme for the `AtomId`
**Status:** Accepted

**Decision.** The `AtomId` is `SHA-256` over a **length-prefixed binary (TLV)
serialization** of the collapsed, normalized atom — no in-band delimiters or
escaping. Layout:

```
[version]                       1 byte (currently 0x01), hashed in-band
[lang-len][lang-bytes]          u32 BE length + UTF-8 BCP-47 tag
( node )*                       zero or more nodes, to end of buffer
  text node:        0x00 [len] [UTF-8 bytes]   (len = u32 BE)
  placeholder node: 0x01                       (no data — D16)
```

- **`u32` big-endian** lengths, written explicitly (`to_be_bytes`), **never**
  native-endian — native-endian would make ids platform-dependent. BE is chosen
  for wire-format convention and hex-dump legibility; correctness is identical
  to LE.
- **No node count** — the framing is self-delimiting, so the byte string decodes
  unambiguously left-to-right. That makes the serialization **injective**
  (distinct collapsed atoms → distinct bytes), which is the whole point.
- A **placeholder contributes only its `0x01` type byte**, so its `data` never
  enters identity while its **position and count do** (D16).
- The **leading version byte is hashed in-band**: a future scheme bumps the
  byte, so two schemes can never produce the same hash input → hard
  cross-version collision separation. This is decisive for §10 (ids shared and
  merged across deployments) and keeps the identity guarantee self-contained in
  this crate rather than delegated to a downstream observation.

**Why TLV over delimiter + escaping.** Text is arbitrary Unicode from XLIFF, so
any in-band sentinel can occur *in the text* and collide with a placeholder
marker; escaping it is the classic source of injectivity bugs. Out-of-band
length framing sidesteps escaping entirely.

**Pipeline.**
`AtomId = SHA-256( serialize( normalize_content( collapse( atom ) ) ) )` —
structural collapse (1b.a, D17) → content normalization (1d, configurable) →
serialize + hash (1b.b). Collapse lands now; serialization + SHA-256 is 1b.b.

---

### D19 — `LanguageTag` validation: parse + required region (script optional), faithful storage
**Status:** Accepted &nbsp;|&nbsp; **Refines D7, D11**

**Decision.**
- Construct via `LanguageTag::parse(s) -> Result<_, LanguageTagError>`. Hard-fail
  if the tag is not well-formed BCP-47 (`Malformed`) or lacks a region subtag
  (`MissingRegion`); both carry the offending tag. Errors are our own type —
  oxilangtag's error stays internal.
- **Required: primary language + region. Optional: script, variants, extensions,
  private-use** — accepted when well-formed, never required. Requiring script
  would reject the normal Suppress-Script form (`en-US`, not `en-Latn-US`).
- **Validity = well-formedness only** (oxilangtag `parse`), no IANA registry
  check (D11); private-use / unusual-but-well-formed tags (`qaa-qm`) are accepted.
- **Faithful storage.** `parse` preserves the original case; lowercasing (and any
  later region-folding) happen in the 1d normalization step, not at construction
  — symmetric with text (D17). So `parse("en-US")` and `parse("en-us")` are
  structurally `!=` but yield the same `AtomId`. (oxilangtag's `==` is itself
  case-sensitive, and its `parse_and_normalize` was rejected precisely because it
  normalizes at construction.)
- Internally wraps `oxilangtag::LanguageTag<String>` (private field) for
  validated subtag accessors and faithful `Eq`; oxilangtag never appears in the
  public API (swappable, §10).
- **Separators: hyphen-only** — oxilangtag rejects `en_US`, so we inherit
  strictness; converting `_`→`-` is the caller's job.

**Why.** Balances strictness (fail loud on malformed / missing region — needed
for determinism and homograph separation, D5) with usability (don't over-require
script). Keeps the gate/normalize split clean: validate at construction,
normalize at identity.

**Note (bi-scriptal languages).** Script need not be required even for Serbian
and the like: differing scripts are different content bytes, so identity already
separates them via content; a caller-supplied script flows into the id faithfully
(`sr-cyrl-rs` ≠ `sr-rs`).

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
  - 1a. IR types (the data model; types only, no behavior). ✓ done.
  - 1b.a. Structural collapse — merge adjacent text, drop empty runs (D17, Q3);
    no dependencies.
  - 1b.b. Canonical serialization (D18) + SHA-256 over it → `AtomId`; adds the
    `sha2` crate.
  - 1d. Content normalization inserted into the `AtomId` pipeline: NF form,
    whitespace, case, and BCP-47 via `oxilangtag` (resolves Q4; implements D7,
    D11).
  - 1e. Invariant tests over the `AtomId` (distinct chunkings → same id; distinct
    structure → distinct id; collapse / normalize idempotence).
- **Phase 2 — XLIFF 1.2 adapter + fidelity gate.** `parse: XLIFF 1.2 → IR`,
  `emit: IR → XLIFF 1.2`, round-trip diff test as a CI gate; language-only-tag
  policy (Q5); `<sub>` extraction (D8).
- **Phase 3+ — Dialects & hard cases.** memoQ / SDL dialects; `<mrk>` (Q2);
  normalization refinements.
