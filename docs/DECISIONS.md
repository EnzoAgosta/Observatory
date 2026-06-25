# Observatory — Decision Log

The reasoning record for Observatory, the normalization core of the translation data layer.

This log is **append-only**, mirroring the system it documents. A decision is never edited in place once **Accepted**; if we change our minds, we add a new decision that **supersedes** the old one and update the old one's status to `Superseded by Dn`. Open questions live in their own section and graduate into decisions when resolved.

**Conventions**

- Decisions are numbered `D1, D2, …`, never renumbered or deleted.
- Open questions are numbered `Q1, Q2, …`. When resolved, mark `Resolved by Dn`.
- Statuses: `Accepted`, `Superseded by Dn`.

---

## Decision Log

### D1 — Scope: Observatory is the normalization engine, nothing else

**Status:** Accepted

**Context.** The wider system (per thesis) has two storage engines — Lance (strings + embeddings, "what's near") and Kùzu (typed observations, "how things relate") — plus retrieval, training, and an eventual business layer. The thesis (§7, §11) identifies the normalization engine — defining the _atom_ and proving it round-trips — as the make-or-break artifact that gates everything downstream. (The two-engine storage shape was refined in D22 — Lance as the single substrate, Kùzu dropped after its archival.)

**Decision.** This project (`observatory`) implements _only_ the normalization core: the intermediate representation (IR), content-addressed identity, and the XLIFF boundary. Lance, Kùzu, embeddings, retrieval, training, and orchestration are explicitly out of scope here.

**Why.** "You can always derive the old TM from this model, but never the reverse" only holds if the atom is defined crisply first. Build the foundation before the floors that rest on it.

---

### D2 — Atom granularity: kind + pairing, no payload

**Status:** Superseded by D16

**Context.** A segment with inline tags is "a string + a document-specific structure" (§7). The same content with different tag encodings (`<g id=3>` vs `<x/>`, bold vs link) must not fragment into different atoms.

**Decision.** The canonical content (the atom) keeps text plus **positional placeholders that carry tag _kind_ and _pairing_ structure, but not payload**. Structure is _content_; the original encoding is _occurrence data_ (a future observation), and never enters identity.

**Consequences.**

- `Click <a href=x>here</a>` and `Click <b>here</b>` → **same atom** (`Click ⟦open:1⟧here⟦close:1⟧`); they differ only in payload.
- `Click {0}here{1}` (two standalones) → **different atom**; the structure genuinely differs.

---

### D3 — Boundary: XLIFF only, and only version 1.2

**Status:** Accepted

**Context.** §8 argues the IR should be strict and narrow, with the messy outside world held at an adapter boundary. The boundary format must be chosen.

**Decision.** XLIFF is the _one and only_ boundary format. We target **XLIFF 1.2 specifically** and do not account for 2.0 / 2.1 / 2.2.

**Why.** Effectively every system in the field still emits 1.2. XLIFF 2.0's inline model (`pc`/`sc`/`ec`) added complexity the market never adopted. Committing to 1.2 makes the engine _more_ precise and optimized, not less. Vendor dialects (memoQ `.mqxliff`, SDL `.sdlxliff`) are XLIFF 1.2 underneath (§8) and slot in as dialects later.

**Closed inline-tag set, mapped to XLIFF 1.2:**

| IR kind      | XLIFF 1.2 elements                                 |
| ------------ | -------------------------------------------------- |
| `Open`       | `<g>` (start), `<bpt>`, `<bx/>`, `<it pos="open">` |
| `Close`      | `<g>` (end), `<ept>`, `<ex/>`, `<it pos="close">`  |
| `Standalone` | `<ph>`, `<x/>`                                     |

Pairing is conveyed by 1.2's `id` / `rid` attributes, captured as our `link`.

---

### D4 — Identity hash: SHA-256

**Status:** Accepted

**Context.** The atom's primary key is a content hash. Candidates: SHA-1 (broken, 160-bit), SHA-256 (standard, 256-bit), BLAKE3 (modern, fast, Merkle-tree based), plus SHA-512/SHA-3 and non-cryptographic hashes (xxHash/Murmur — disqualified, a collision in a primary key silently merges two atoms).

**Decision.** **SHA-256.**

**Why.** Inputs are short segments, so throughput is irrelevant (XML parsing will dominate, and BLAKE3's parallelism only helps on large inputs). The deciding axis is that this ID is part of an **open standard meant to be adopted**: SHA-256 is reproducible in any language with one line and zero exotic dependencies (`sha256sum` exists everywhere), with no "what's BLAKE3?" friction.Revisit only if hashing is ever _measured_ to be a bottleneck.

---

### D5 — Identity composition: normalized content + canonical language tag, nothing else

**Status:** Accepted

**Decision.** `identity = SHA-256(normalized_unicode_content ‖ canonical_BCP-47_language_tag)`. No other field is fed to the identity function.

**Guiding principle.** _Include in identity only what makes two byte-identical strings genuinely different atoms_. Everything that is a fact _about_ an atom rather than _constitutive_ of it is an observation.

- **Language** passes: `"Gift"` (en) and `"Gift"` (de) are different translatable objects sharing bytes; every downstream op (embedding, retrieval, dedup) is per-language.
- **Direction (source vs. target)** fails — this is the entire thesis; direction is the `TRANSLATION_OF` observation, never identity.
- **Domain, register, client, provenance, recency** all fail — textbook observations.
- **Tag payload** already excluded by D2.

Languages are represented in **BCP-47**, canonicalized for identity (case/script normalization per the registry). The exact byte layout of `‖` (the concatenation/serialization fed to SHA-256) is to be designed in Phase 1.

---

### D6 — Flexibility lives in normalization, not in the hash

**Status:** Accepted

**Context.** Considered making the hash itself pluggable so implementers could choose how to differentiate strings.

**Decision.** The **identity formula is fixed** (D4 + D5). **Normalization is the configurable surface**, exposed as _named profiles_: how Unicode content is normalized (NF form, whitespace, case) and how BCP-47 tags are normalized (e.g. whether `en-US` and `en-GB` fold together). We ship a canonical default profile.

**Why.** A content-addressed ID's value is that it is **globally reproducible** — same string → same ID in every deployment, so graphs merge, corpora are shareable, and there is a canonical public ID. Free-form hashing destroys that (my IDs ≠ your IDs), is a foundational footgun, and is premature generalization (§10's whole bet is to make this model _the standard_). Moving flexibility one layer down to normalization preserves ~90% of the desired control while keeping one reproducible hashing scheme. Same profile → shareable IDs for free; divergence is a deliberate, _named_ choice. If truly alternate identity schemes are ever needed, add _named_ schemes then — not on spec.

---

### D7 — Language tag: lowercase, region mandatory, no folding, external parser

**Status:** Accepted &nbsp;|&nbsp; **Resolves Q1**

**Decision.**

- BCP-47 tags fed to identity are normalized to **all lowercase**.
- A **region subtag is mandatory**. A tag without a region **fails loud and fast** (validation error) — we never guess or default one.
- **No region folding**: `en-us` ≠ `en-gb`, `nl-be` ≠ `nl-nl`. Region variants differ in orthography, terminology, and register; they are different atoms.
- BCP-47 **parsing/validation uses an external crate**, not a hand-rolled parser. (Exact crate chosen in Phase 1d.)

**Notes.**

- All-lowercase is _our_ deterministic identity form, deliberately simpler than BCP-47's recommended case form (language lower / script Title / region UPPER). BCP-47 is case-insensitive, so any single deterministic casing is a valid identity key; lowercase is the simplest.
- Robust BCP-47 handling (grammar + IANA registry rules) is non-trivial; leaning on a vetted parser beats reimplementing it.

**Consequence → Q5.** XLIFF 1.2 frequently carries _language-only_ tags (`source-language="en"`). "Region mandatory" means such tags cannot form an identity as-is; resolving the region becomes a layer-above responsibility.

---

### D8 — `<sub>` is a layer-above concern; identity stays oblivious

**Status:** Accepted &nbsp;|&nbsp; **Resolves the `<sub>` half of Q2**

**Decision.** The identity/IR layer does **not** special-case `<sub>` (translatable text nested inside a code). The application/adapter layer extracts sub-content as its **own string with its own identity and observations** — and "this string was a sub-segment of X" is itself just another observation. The identity function never needs to know `<sub>` exists.

**Why.** Consistent with the whole model: anything that looks like it wants to be a special case _on_ the atom is usually an observation a layer up. Extraction is an adapter behavior (Phase 2+), not an identity concern.

---

### D9 — Testing is a first-class citizen

**Status:** Accepted

**Decision.** Every function we create is tested. We are **not** doing strict TDD (tests need not come first), but no function ships untested. Test layers:

- **Unit tests** colocated per module (`#[cfg(test)] mod tests`).
- **Integration tests** in `tests/` for cross-module behavior (e.g. round-trip).
- **Property tests** for invariants (normalization idempotence, structural distinction, hash determinism).
- **Snapshot tests** for the canonical serialization and XLIFF round-trip fidelity.

(Property/snapshot tooling is a dependency choice — see the Phase 0 dependency proposal.)

---

### D10 — Crate layout: single library crate for now

**Status:** Accepted

**Decision.** `observatory` is a **library crate** (no binary) with modules `ir`, `identity`, `normalize` (and `xliff` in Phase 2). Promote to a Cargo **workspace** (e.g. `observatory-core` + `observatory-xliff` + a CLI) only when a concrete need justifies the split — not pre-emptively.

**Why.** It is the reusable open-core primitive (D1), so a library is the right shape. KISS: one crate with clear module boundaries; split later if it earns it.

---

### D11 — BCP-47: enforce well-formedness, not registry validity; crate `oxilangtag`

**Status:** Accepted &nbsp;|&nbsp; **Refines D7**

**Decision.** The identity layer requires a language tag that is **well-formed** (matches the BCP-47 grammar) **with a region subtag present** (D7), and canonicalizes it to lowercase. It does **not** check **validity** (whether each subtag exists in the IANA registry). Parsing uses **`oxilangtag`** (Oxigraph project): well-formedness only, with subtag accessors, no bundled registry.

**Why.**

- _Validity is policy, and policy lives above the fixed identity layer (D6)._ "Which tags count as real" is a caller/profile decision, not an identity one.
- _A legitimate use case needs it:_ someone may deliberately use a synthetic or private locale (e.g. `qaa-zz`, or an `x-…` private-use tag) to force separate storage / control dedup. A library that rejected such tags would impose policy it has no business imposing.
- _The clean, predictable line is "well-formed + region present," not "registry alid."_ Note BCP-47 already reserves registry-valid private-use ranges `qaa`–`qtz`; regions `ZZ`, `QM`–`QZ`, `XA`–`XZ`; the `x-…` subtag), so even trict validation wouldn't behave as one might naively expect.

If registry validation is ever wanted, it becomes an optional normalization-profile or app-layer check (D6 territory) — never baked into identity.

---

### D12 — License: dual `MIT OR Apache-2.0`

**Status:** Accepted

**Decision.** License the crate **`MIT OR Apache-2.0`** (the Rust-ecosystem defa ult), shipping both `LICENSE-MIT` and `LICENSE-APACHE` at the repo root anddeclaring the SPDX expression in `Cargo.toml`.

**Why.** Permissive, matching §10's open-core intent. Dual-licensing costs ssentially nothing (two text files) while letting downstream users pick MIT's implicity or Apache-2.0's explicit patent grant + retaliation clause, and aximizes compatibility (e.g. GPLv2 projects can consume the MIT side). Copyright older recorded as "Enzo Agosta, 2026" — adjust if a different attribution is anted.

---

### D13 — The canonical unit is named `Atom` (not `Segment`)

**Status:** Accepted

**Decision.** The content-addressed unit is the **`Atom`**; its content hash is the **`AtomId`**.

**Why.** "Segment" is ubiquitous in the translation industry and carries conventional baggage — it implies a source→target row in a TM. This project deliberately _decouples_ strings from relationships (D1), so naming the unit `Atom` reinforces that it is a single, relation-free, content-addressed string, not a TM segment.

---

### D14 — Identity is a dumb, reversible recording: positional renumbering, permissive structure

**Status:** Accepted

**Decision.**

- Building an `Atom` is a **dumb, reversible recording** of "what is text and what is tag," in order. The IR does **not** validate the input. Assumption: any XLIFF segment fed to us was _already valid_ upstream.
- Tags are **always renumbered to appearance (positional) order**, regardless of the source's original ids — even if those ids were "correct." Original dialect ids are preserved in `payload` for round-trip.
- **Permissive on structure**: we do not enforce well-nested pairing (XLIFF 1.2 allows overlap via `rid`). We require only canonical positional numbering; whatever pairing the input expresses, we record it and can reverse it.

**Why.** Round-trip fidelity demands we accept whatever valid XLIFF contains; enforcing well-nesting would risk lossy round-trips and is policy that belongs above identity (cf. D6). Canonical positional numbering is precisely what makes the atom stable across differing dialect id schemes (D2).

---

### D15 — `Payload` stays opaque for Phase 1

**Status:** Superseded by D16

**Decision.** `Payload` — the recoverable, dialect-specific tag data that is never hashed (D2) — is modeled as a thin **opaque blob** for Phase 1 (format-free). Its internal structure is deferred to Phase 2, when XLIFF 1.2 round-trip requirements are concrete.

**Why.** It never affects identity, so its shape should follow the round-trip needs we will only know once we parse real XLIFF. Avoids speculative structure (KISS).

---

### D16 — Uniform placeholders: identity by position and count, not kind

**Status:** Accepted &nbsp;|&nbsp; **Supersedes D2 and D15; refines D14**

**Context.** D2 had the atom keep placeholder _kind_ (open / close / standalone) and pairing in identity. But encoding kind requires _interpreting_ each placeholder's role — exactly the interpretation D14 says this layer must not do — and the thesis (§7) lists a paired `<g>` vs a standalone `<ph/>` as "the same content, different encodings" that should _not_ fragment.

**Decision.**

- A content node is one of exactly **two** kinds: **Text** or **Placeholder**. No open / close / standalone distinction is recorded.
- A placeholder is **opaque**: its raw original markup is stored as the node's `data` and is never interpreted. (This folds D15 — there is no separate `Payload` type; the placeholder's `data` _is_ the opaque payload.)
- **Identity distinguishes placeholders only by position and count**, never by kind or content. The placeholder's `data` never enters the hash (D2's no-payload-in-identity rule survives); each placeholder contributes a single uniform canonical marker to the hashed projection.
- **Reconstruction is the in-order join of every node's `data`** — a blind, lossless concatenation (the reversible half of D14).

**Consequence (accepted consciously).** Structurally different taggings of identical text collapse to one atom: `Click <b>here</b>` (a pair) and `Click {0}here{1}` (two standalones) share an `AtomId`. The translatable text is identical; the tagging difference is occurrence data recoverable from `data`, and "what kind of placeholder" — if ever wanted — is a derivation one layer up, never part of identity. Round-trip is unaffected (it uses raw `data`, not identity).

**Model.**

- `Atom { language, content: Vec<ContentNode> }` — order is significant.
- `ContentNode { kind: ContentKind, data: String }` — `data` is raw, UTF-8 (XLIFF is XML text, D3).
- `ContentKind { Text, Placeholder }`.
- Canonical-by-construction reduces to: **merge adjacent Text, drop empty Text** (no links to renumber anymore).

---

### D17 — Faithful construction; structural normalization lives in the `AtomId`

**Status:** Accepted &nbsp;|&nbsp; **Refines D14; supersedes the "canonical-by-construction" clause and model sketch of D16**

**Context.** D16 had the `Atom` be _canonical by construction_ — `Atom::new` merged adjacent text and dropped empty runs. That makes construction silently mutate the recording.

**Decision.**

- **Construction is a faithful, non-normalizing recording.** `Atom::new` stores exactly the nodes it is given. `[Text("a"), Text("b")]` stays distinct from `[Text("ab")]`; empty and adjacent runs are preserved. Construction carries zero logic.
- **Structural normalization — merge adjacent text, drop empty text — moves into the `AtomId` computation** (the hashing projection, Phase 1b–1c) as an explicit, independently-tested function. So `[Text("a"), Text("b")]` and `[Text("ab")]` are **different `Atom`s** that produce the **same `AtomId`**.
- This keeps the no-fragmentation guarantee (normalization still happens before hashing) while keeping the recording pure.

**Consequence (accepted; documented on the types).** Structural equality (`==`) is a _different relation_ from identity equality. Two `Atom`s may share an `AtomId` without being `==`. **Dedup and identity are always via `AtomId`, never `==`**; `==` means "structurally identical recording."

**Why.** A pure, surprise-free constructor is the strongest round-trip-safe contract and the truest expression of "dumb recording" (D14); identity stays a deliberate, derived projection — the `AtomId` is a _materialized view_ over the raw `Atom`, consistent with the thesis's raw-data-plus-views model.

**Model (updated; supersedes D16's sketch).**

- `Atom { language, content: Vec<ContentNode> }` — order significant; faithful, non-normalizing.
- `ContentNode { is_placeholder: bool, data: String }` — the closed binary is a `bool`, not an enum (`ContentKind` dropped). Constructors `text()` `placeholder()`; accessor `is_placeholder()`
- Reconstruction = in-order join of `data`. Normalization for hashing is a separate Phase 1b function.

---

### D18 — Canonical serialization scheme for the `AtomId`

**Status:** Accepted

**Decision.** The `AtomId` is `SHA-256` over a **length-prefixed binary (TLV) serialization** of the collapsed, normalized atom — no in-band delimiters or escaping. Layout:

```
[version]                       1 byte (currently 0x01), hashed in-band
[lang-len][lang-bytes]          u32 BE length + UTF-8 BCP-47 tag
( node )*                       zero or more nodes, to end of buffer
  text node:        0x00 [len] [UTF-8 bytes]   (len = u32 BE)
  placeholder node: 0x01                       (no data — D16)
```

- **`u32` big-endian** lengths, written explicitly (`to_be_bytes`), \*_never_ native-endian — native-endian would make ids platform-dependent. BE is chose for wire-format convention and hex-dump legibility; correctness is identica to LE.
- **No node count** — the framing is self-delimiting, so the byte string decodes unambiguously left-to-right. That makes the serialization **injective** (distinct collapsed atoms → distinct bytes), which is the whole point.
- A **placeholder contributes only its `0x01` type byte**, so its `data` never enters identity while its **position and count do** (D16).
- The **leading version byte is hashed in-band**: a future scheme bumps the byte, so two schemes can never produce the same hash input → hard cross-version collision separation. This is decisive for §10 (ids shared and merged across deployments) and keeps the identity guarantee self-contained in this crate rather than delegated to a downstream observation.

**Why TLV over delimiter + escaping.** Text is arbitrary Unicode from XLIFF, so any in-band sentinel can occur _in the text_ and collide with a placeholder marker; escaping it is the classic source of injectivity bugs. Out-of-band length framing sidesteps escaping entirely.

**Pipeline.**
`AtomId = SHA-256( serialize( normalize_content( collapse( atom ) ) ) )` — structural collapse (1b.a, D17) → content normalization (1d, configurable) → serialize + hash (1b.b). Collapse lands now; serialization + SHA-256 is 1b.b.

---

### D19 — `LanguageTag` validation: parse + required region (script optional), faithful storage

**Status:** Accepted &nbsp;|&nbsp; **Refines D7, D11**

**Decision.**

- Construct via `LanguageTag::parse(s) -> Result<_, LanguageTagError>`. Hard-fail if the tag is not well-formed BCP-47 (`Malformed`) or lacks a region subtag (`MissingRegion`); both carry the offending tag. Errors are our own type — oxilangtag's error stays internal.
- **Required: primary language + region. Optional: script, variants, extensions, private-use** — accepted when well-formed, never required. Requiring script would reject the normal Suppress-Script form (`en-US`, not `en-Latn-US`).
- **Validity = well-formedness only** (oxilangtag `parse`), no IANA registry check (D11); private-use / unusual-but-well-formed tags (`qaa-qm`) are accepted.
- **Faithful storage.** `parse` preserves the original case; lowercasing (and any later region-folding) happen in the 1d normalization step, not at construction — symmetric with text (D17). So `parse("en-US")` and `parse("en-us")` are structurally `!=` but yield the same `AtomId`. (oxilangtag's `==` is itself case-sensitive, and its `parse_and_normalize` was rejected precisely because it normalizes at construction.)
- Internally wraps `oxilangtag::LanguageTag<String>` (private field) for validated subtag accessors and faithful `Eq`; oxilangtag never appears in the public API (swappable, §10).
- **Separators: hyphen-only** — oxilangtag rejects `en_US`, so we inherit strictness; converting `_`→`-` is the caller's job.

**Why.** Balances strictness (fail loud on malformed / missing region — needed for determinism and homograph separation, D5) with usability (don't over-require script). Keeps the gate/normalize split clean: validate at construction, normalize at identity.

**Note (bi-scriptal languages).** Script need not be required even for Serbian and the like: differing scripts are different content bytes, so identity already separates them via content; a caller-supplied script flows into the id faithfully (`sr-cyrl-rs` ≠ `sr-rs`).

---

### D20 — Normalization profile: explicit input, not part of the `AtomId`

**Status:** Accepted &nbsp;|&nbsp; **Resolves Q4**

**Model.** A small, transparent struct of enums (D6):

- `NormalizationProfile { unicode: UnicodeForm, whitespace: WhitespacePolicy }`
- `UnicodeForm { Nfc, Nfkc }`, `WhitespacePolicy { Preserve, TrimOuter }`
- `NormalizationProfile::DEFAULT = { Nfc, TrimOuter }`.

Knobs are deliberately few; more (inner-whitespace collapse, region-fold, …) are added only as need is shown.

**Explicit, never implicit.** `atom_id` / `canonical_bytes` take a &NormalizationProfile`argument — there is no implicit default. Normalization is ntegral to how an id is derived, so it must be visible at the call site; callers ass`NormalizationProfile::DEFAULT` to opt into our defaults.

**Not part of the hash.** The profile is _not_ serialized into the hash input. Its entire effect is already in the normalized bytes: if two profiles yield the same normalized content they must produce the same id (else identical content fragments, §7); if they yield different content the ids already differ. Hashing the profile would be redundant-or-harmful, and would churn _every_ id on any knob change (vs. only the atoms actually affected).

_Contrast with the serialization version (D18), which IS hashed:_ the version separates byte-**layout** ambiguity (different atoms could collide to identical bytes — silent); a profile is a content **transformation** whose result is self-evident in the bytes. Layout needs in-band separation; transformation does not.

**Provenance is storage-layer metadata.** "Which profile produced this id" is worth recording — but as metadata next to the id in the storage layer (out of scope here), never inside the id.

**Default rules.** NFC (lossless, standard); trim **outer** whitespace only — the leading edge of the segment's first text run and the trailing edge of its last, with internal / placeholder-adjacent whitespace preserved; language tag lowercased (D7, fixed); region-folding deferred.

**Consequence.** Cross-deployment `AtomId` sharing (§10) requires agreeing on he serialization version and the normalization profile — a documented recondition that cannot be self-enforced by the id without breaking ingle-profile dedup. (The full precondition is sharpened in D21.)

---

### D21 — Closing the reproducibility gap: explicit edge-trim set, pinned Unicode version

**Status:** Accepted &nbsp;|&nbsp; **Refines D20**

**Context.** A cold audit found that the cross-implementation reproducibility promise (§10) had two _unpinned_ dependencies silently determining the `AtomId` bytes: the definition of "whitespace" used for trimming (Rust's Unicode `White_Space`, which differs across languages and std versions), and the Unicode version behind NFC/NFKC. D20's stated precondition ("serialization version + profile") omitted both — the profile was treated as the full normalization spec, but it was only a _selector_ over ambient library behavior.

**Decision.**

- **The whitespace knob becomes an explicit set of code points** on the profile: `NormalizationProfile.edge_trim: Vec<char>`. Trimming removes leading and trailing runs of characters in this set from a segment's outer edges, stopping at the first character not in the set. An empty set trims nothing (replacing the old `WhitespacePolicy::Preserve`); the default set is `{U+0009, U+000A, U+000D, U+0020}` (replacing `TrimOuter`). This makes the trimmed set explicit, customizable (even non-whitespace, or per-language), debuggable, and reproducible — with no dependence on any library's whitespace definition. As a bonus, NBSP and other Unicode spaces are _not_ trimmed by default, so semantically meaningful spacing survives.
- **The Unicode normalization version is part of the contract.** unicode-normalization` is pinned to an exact version (`=x.y.z`) so its Unicode ata changes only through a deliberate, revertible dependency bump.

**The full reproducibility contract** (supersedes D20's two-item version): two deployments produce identical `AtomId`s for the same input iff they agree on (1) the serialization version, (2) the normalization profile _including its edge-trim set_, and (3) the Unicode version of the normalization tables.

**Why.** An `AtomId` is only globally reproducible if every input to the hash is pinned by the spec, not by ambient library behavior. The audit showed two such inputs were implicit; this makes them explicit.

### D22 — Wider-system storage: Lance as the single substrate, DuckDB as query engine; Kùzu dropped

**Status:** Accepted &nbsp;|&nbsp; **Refines the storage clause of D1**

**Context.** D1's context described the wider system as two storage engines — Lance (strings + embeddings) and Kùzu (typed observations) — mirroring the thesis's "what's near" / "how things relate" split. Since then Kùzu was acquired by Apple (deal disclosed Oct 9, 2025) and the `kuzudb/kuzu` repository was archived the next day with a "working on something new" note — an acquihire, not neglect. The technology is not coming back as a maintained independent project. (A community fork, `Vela-Engineering/kuzu`, exists as of Feb 2026 but is too small to bet an architecture on.)

**Decision.**

- **Lance is the single storage substrate** for the wider system: atoms, embeddings, and the observation log all live as Lance tables. Lance's columnar scans, random access, and built-in vector indexes cover the dominant translation workloads (corpus scans, per-atom lookup, semantic nearness) in one engine.
- **DuckDB is a query engine over Lance data**, not a store. It handles the 1–2 hop joins and filtered scans that make up the common case — e.g. "all translations of this string approved by a human and not blacklisted, as of ≤3 months ago" — for free.
- **The graph is a derived view, not a store of record.** Observations are an append-only log (subject atom, predicate, object/value, timestamp, provenance); "the graph" is a projection built when a query needs it, consistent with the event-sourcing/lakehouse shape the system has always had.
- **Kùzu is dropped.** A real graph engine is only re-evaluated if a concrete _recursive_ query (transitive `context_for`, multi-hop review reachability) proves unserveable by Lance + DuckDB — not on architectural aesthetics. The Kùzu fork remains a safety valve if that day comes.

**Why.** The "what's near" / "how things relate" split was always an access-pattern split, not a mandate for two engines. Lance's random access plus DuckDB's columnar joins cover the 1–2 hop access pattern the common queries need; a separate graph engine earns its place only at recursion, which is not the common case. Collapsing to one substrate also keeps the system honest to the lakehouse/event-sourcing model — one append-only log, many derived views — rather than bifurcating truth across two stores.

**Consequence for the observation layer (deferred, recorded so it isn't lost).** With the graph as a derived view, the observation layer's design reduces to: a uniform append-only log as the source of truth + materialized per-predicate projections for hot read paths (the TM projection being the obvious first one). Cross-deployment sharing, which at the atom layer is solved by the `AtomId` standard (§10), hits a new standardization surface at the observation layer: predicate semantics. A small core shared vocabulary (`TRANSLATION_OF`, review, provenance) plus a namespaced extension space is the likely shape, but this is designed _after_ the XLIFF adapter (Phase 2) has produced concrete ingest experience — not before. Designing the read model before seeing the writes would reproduce the mistake the atom layer was specifically careful to avoid.

### D23 — Commit `Cargo.lock` for the library crate

**Status:** Accepted &nbsp;|&nbsp; **Refines D21**

**Decision.** `Cargo.lock` is committed to the repository (not gitignored), despite the older Rust convention of omitting it for libraries.

**Why.** A library's `Cargo.lock` does **not** affect downstream consumers — Cargo ignores it when the crate is used as a dependency; it only governs this crate's own dev and CI builds, so the usual cost of committing it for a library (constraining a consumer's dependency graph) does not exist here. What it buys is exactly D21's value: the lock file is the mechanism that enforces the exact pin on `unicode-normalization` (and pins transitive deps) at build time, so two CI runs can't silently resolve to different patch versions and drift the `AtomId` bytes in our own tests. The "don't commit for libraries" advice is increasingly considered outdated and never traded against the reproducibility cost it was meant to save. Recorded so the choice is deliberate, not accidental, and so a future "shouldn't this be gitignored?" PR has an answer.

### D24 — Promote to a Cargo workspace: one repo, multiple crates

**Status:** Accepted &nbsp;|&nbsp; **Enacts D10's deferred split**

**Context.** D10 scoped `observatory` as a single library crate and said it would be promoted to a Cargo workspace "only when a concrete need justifies the split — not pre-emptively." The XLIFF 1.2 adapter (Phase 2) is that need: it brings an XML parser dependency and dialect-specific concerns that must not leak into the normalization core, and it should be independently releasable.

**Decision.** The repo becomes a **Cargo workspace** of multiple crates, developed and tested together but released independently:

```
Cargo.toml              workspace manifest (no [package])
Cargo.lock              one lockfile for the whole workspace

crates/
  observatory/          the normalization core (this crate, renamed in place)
  observatory-xliff/    the XLIFF 1.2 adapter (Phase 2)
  observatory-lance/    Lance storage substrate (later)
```

- The **root `Cargo.toml` has no `[package]`** — it is a workspace manifest listing `members = ["crates/*"]`.
- A **`[workspace.package]`** table sets shared metadata (edition, license) once; a **`[workspace.dependencies]`** table pins shared deps (e.g. the D21 exact pin on `unicode-normalization`) in one place that every member crate inherits.
- Each member crate has its **own `[package]` Cargo.toml** with its own name and version, inheriting shared metadata via `.workspace = true`.
- **Cross-crate deps use both `path` and `version`** — `path` so local changes build against local source without publishing, `version` so crates.io consumers resolve correctly. Never one without the other.
- One `Cargo.lock` and one `target/` shared across the workspace; `cargo test` at the root builds and tests every crate, `cargo test -p observatory-xliff` scopes to one. D23's "commit the lockfile" stance is unchanged (there is now one workspace lockfile).
- Each crate is **released independently**: `cargo publish -p observatory`, then later `cargo publish -p observatory-xliff`, each with its own version. A consumer who only needs the core pulls `observatory` and never gets the XML parser.

**Naming.** The core keeps the bare name `observatory` (ecosystem convention: `tokio` vs `tokio-util`, `serde` vs `serde_json`); adapters/extensions get the `-<role>` suffix. No separate repos — atomic cross-crate changes (e.g. adding a `region()` accessor to `LanguageTag` for the XLIFF adapter's Q5 work) land in one commit and are tested together, which separate repos make painful.

**Why now.** Converting before writing the XLIFF adapter means Phase 2 starts in the right shape, instead of bolting it onto the single crate and untangling later. The conversion is mechanical (move `src/`, `tests/`, and the crate-level `Cargo.toml` into `crates/observatory/`, add the workspace manifest and an `observatory-xliff` skeleton), and `cargo test` at the root confirms nothing broke.

### D25 — Crate naming: `observatory-core` + `observatory-xliff12`; per-crate READMEs

**Status:** Accepted &nbsp;|&nbsp; **Refines the naming clause of D24**

**Context.** D24's naming clause followed the ecosystem convention of giving the core the bare project name (`tokio`, `serde`). But "observatory" is the name of the _system_ and the _repo_ — using it for one member crate shadows the whole project and makes "observatory the system" vs "observatory the crate" ambiguous in prose, docs, and dependency lists. Separately, D3 commits hard to XLIFF 1.2 only, and that scope is invisible at the dependency line if the adapter is named plain `observatory-xliff`.

**Decision.**

- **Core crate: `observatory-core`** (directory `crates/observatory-core/`). The name already matches the crate description ("Normalization core for a translation data layer") and D1's "normalization engine." In code: `use observatory_core::ir::Atom`.
- **Adapter crate: `observatory-xliff12`** (directory `crates/observatory-xliff12/`). crates.io forbids dots, so `12` is the cleanest encoding of "1.2"; the name makes the 1.2-only contract (D3) visible at the dependency line and leaves room for a future `observatory-xliff2` rather than overloading one crate.
- **Cross-crate dependency updated**: the adapter's dependency becomes `observatory-core = { path = "../observatory-core", version = "0.1.0" }`.

**Per-crate READMEs.**

- **Root `README.md`** describes Observatory as a _system_ — the thesis (atoms as content-addressed nodes, observations as append-only facts, the decoupling, the event-sourcing/lakehouse shape, Lance as substrate, DuckDB as query engine), the workspace layout, and pointers to each crate's README and to `docs/DECISIONS.md`. No code example here; the tested example lives in the core crate's README.
- **`crates/observatory-core/README.md`** describes this crate: the atom IR, `AtomId`, normalization, the pipeline, and the tested code example (importing `observatory_core::…`). This is the current root README content, relocated.
- **`crates/observatory-xliff12/README.md`** describes this crate: D3 (XLIFF 1.2 only), the closed inline-tag set, how it relates to `observatory-core`, Q5 (language-only tags) as open scope, and current status (skeleton).

**Why.** A system repo where one member crate shadows the project name is a permanent ambiguity in every doc, issue, and `Cargo.toml` that references it; `-core` is the conventional disambiguation (cf. `bevy_core`, `tokio-util`). Encoding the XLIFF version in the adapter name surfaces D3's hard 1.2-only commitment at the dependency line rather than only in the decision log, and preserves a clean naming lane for a hypothetical future 2.x adapter. The per-crate READMEs match the repo's own "describe what each thing is, keep relationships separate" shape: the root is the thesis, each crate documents itself.

---

### D26 — The XLIFF 1.2 adapter is a stateless content-node codec: spec-driven tokenization, logical/verbatim entities

**Status:** Accepted &nbsp;|&nbsp; **Refines D3, D16; relates to D8, Q2, Q5**

**Context.** D24/D25 created `observatory-xliff12` as the boundary crate. Its scope is now fixed far narrower than the phase plan implied: a _pure, stateless gate_ between one XLIFF 1.2 content fragment and an `Atom` — two functions, no document model, no validation, no `<file>` / `<trans-unit>` traversal, no source↔target pairing, no language resolution. Deciding _which_ node becomes an atom, validating the document, and recording relationships (`TRANSLATION_OF`, sub-of) are consumer concerns (D1). This crate is a primitive, not an app.

**Decision.**

_Surface._ `parse(content, language, codec) -> Result<Atom, _>` tokenizes the XML string of an assumed-valid, already-extracted content node into an `Atom`; the caller supplies the `LanguageTag` (it lives a level up on `<file>`, and requiring it gates the caller's own validity). `emit(atom, codec) -> String` is the inverse. No other configuration.

_Tokenization is purely spec-content-model-driven._ The only per-element decision is what XLIFF 1.2 declares the content model to be:

- **Content is native code** (`<bpt>`, `<ept>`, `<ph>`, `<it>`): read to the element's end; record the _entire_ span — tag, code, any `<sub>`, close — as **one opaque placeholder**. We never look inside, so `<sub>` presence is irrelevant (D8 — extraction is a layer above).
- **Content is translatable text** (`<g>`, `<mrk>`): record the element's _presence_ (open tag with all attributes as a placeholder, close tag as a placeholder) and **keep tokenizing the inner content as text and nested inlines** (recursion, same rule). Text stays in the atom; markup becomes position+count placeholders (D16).
- **Empty elements** (`<x/>`, `<bx/>`, `<ex/>`): the same "not code" branch, degenerate — a single presence placeholder, no inner text.

The whole rule: _spec says code → hide it; spec says text → keep the text, mark the tag._ No interpretation of ids, rids, pairing, or sub-flows.

_Placeholders are raw byte-slices of the original input, never re-serialized_ — this keeps attribute order, quoting, and `<x/>`-vs-`<x></x>` faithful.

_Entity codec — logical (default) or verbatim,_ passed symmetrically to `parse` and `emit`:

- **Logical (default):** text runs are XML-unescaped to Unicode on parse and re-escaped on emit with partial escaping (`<`, `>`, `&`; quotes untouched). Round-trip is **content-identical**; identity is correct — fragments differing only in escaping share an `AtomId` (the §10 bet). Placeholder bytes are never decoded.
- **Verbatim:** text runs kept as raw bytes; emit concatenates. Round-trip is **byte-identical**, but identity hashes the escaped form — a deliberate, named caller choice (D6).
- The mode is not stored on the atom, so `parse` and `emit` must use the _same_ codec; the codec object makes that symmetry natural.

_Failure is loud._ Unknown/custom (DTD) entities under logical mode — outside the standard XML set — are a parse error, never silently passed through.

**Deferred (unchanged).** `<mrk>` semantics (Q2): for now tokenized exactly like `<g>` (presence marked, text kept), which loses nothing and round-trips; whether its boundary should become _transparent_ in identity, or a caller policy, stays open. `<sub>` extraction (D8), language-only tags (Q5), and all document-level concerns remain the consumer's.

**Why.** The boundary crate's one job is faithful translation between XLIFF content and atoms; every choice beyond "is this content code or text per spec" is interpretation that belongs above the primitive (D1, D6). A spec-driven tokenizer with raw-slice placeholders is the dumbest thing that is also _correct_ for identity and round-trip, and it gets smarter only when a real file proves it must — never on spec.

---

### D27 — `ContentNode` is an enum again; access by exhaustive match

**Status:** Accepted &nbsp;|&nbsp; **Supersedes D17's model clause (`is_placeholder: bool`); restores D16's `ContentKind` enum shape**

**Context.** D16 modeled a content node as a two-variant enum; D17, chasing a dumb constructor, flattened it to `ContentNode { is_placeholder: bool, data: String }` with `is_placeholder()` / `data()` accessors. In practice the bool-plus-payload shape is the exact thing enums exist to replace: every consumer that cares about the kind has to remember to pair `is_placeholder()` with `data()`, and the compiler can't enforce exhaustive handling.

**Decision.**

- `ContentNode` is `enum { Text(String), Placeholder(String) }`.
- Constructors `text()` / `placeholder()` stay (`impl Into<String>`).
- Access is by `match`. A single `as_str()` returns the inner string for either variant — the only shared accessor, used by `reconstruct()` and any kind-agnostic join.
- The `is_placeholder()` and `data()` accessors of D17 are **removed**. Code that branches on kind matches the enum; code that only wants the bytes calls `as_str()`.

**Why.** The kind is a closed binary carrying a payload — the canonical case for an enum. Matching is exhaustive and self-documenting. D17's concern was construction-time mutation, not the field shape; faithful construction (D17) is untouched — the enum just names the two states directly.

**Consequence (ripple).** `collapse` / serialize (identity), `normalize_content` / trim (normalize), and the XLIFF `emit` branch on `is_placeholder()` + `data()` today; each moves to a `match` (or `as_str()` where kind is irrelevant). The `==`-vs-`AtomId` distinction (D17) is unchanged.

---

### D28 — `LanguageTag` deliberately exposes `oxilangtag`; construction is explicit, not `.parse`

**Status:** Accepted &nbsp;|&nbsp; **Supersedes D19's "oxilangtag stays internal / swappable" clause and the `parse` constructor name**

**Context.** D19 wrapped `oxilangtag` entirely: one constructor `LanguageTag::parse`, a private inner field, and a `LanguageTagError` that re-expressed the failure in our own terms so the dependency "never appears in the public API (swappable, §10)." Two things pushed against that wall: the wrapped error threw away the parser's own diagnostic, and a caller who _already_ holds a parsed `oxilangtag` tag had no way to reuse it.

**Decision.**

- **Two constructors, both explicit:**
  - `from_string(impl Into<String>) -> Result<Self, LanguageTagError>` — parse a raw string, then enforce the region rule (D19).
  - `from_parsed(OxiLanguageTag<String>) -> Result<Self, LanguageTagError>` — accept an already-parsed tag and only check the region; skips re-parsing.
- **`as_parsed() -> &OxiLanguageTag<String>`** exposes the validated inner tag, giving callers oxilangtag's subtag accessors (`region()`, `script()`, …) without us re-surfacing each one.
- **`LanguageTagError::Malformed` carries the underlying `oxilangtag::LanguageTagParseError`.** `oxilangtag` is now part of observatory-core's public API.
- **Constructor name is `from_string`, deliberately not `parse` / `FromStr`.** Building an `Atom` is the core act of the whole crate; callers should name _how_ they mint a language tag (`from_string` vs `from_parsed`) rather than lean on an implicit `.parse()`.

**Why.**

- _Debuggability._ The original parse error is the single most useful artifact when a tag is rejected; discarding it (D19) traded real diagnostic value for purity.
- _Reuse / power users._ Most consumers reach core _through_ the XLIFF adapter, which itself benefits from oxilangtag access (e.g. Q5 region resolution). A caller already on oxilangtag can hand us a parsed tag via `from_parsed` (faster — region check only) or reach the original `OxiLanguageTag` via `as_parsed` when they need full subtag detail.
- _Explicitness._ Naming the constructor after its input keeps tag creation — and therefore `Atom` creation — legible at the call site.

**Consequence (accepted consciously).**

- **§10 swappability of the language parser is given up.** Replacing `oxilangtag` is now a breaking change to the public API. Judged worth it: the adapter and core both want oxilangtag, so the abstraction was guarding a swap that isn't going to happen.
- **`LanguageTagError` loses `Clone, PartialEq, Eq`** — `LanguageTagParseError` is `Debug`-only, so the embedded error caps what the enum can derive. Error values can be inspected (`Debug`, `Display`, `matches!`) but not compared or cloned.
- All current call sites use `LanguageTag::parse`; each renames to `from_string`.

**Note.** The region rule, faithful (case-preserving) storage, well-formedness-not-registry validity, and hyphen-only separators of D19 are all unchanged — this decision revises only the constructor surface and error type, not the validation policy.

---

### D29 — Identity is a pure Atom → bytes hash; normalization is a separate, caller-applied step

**Status:** Accepted | **Supersedes D5, D20; reaffirms D6; amends D18**
**Context.** D5/D20 made the `AtomId` a hash over a normalized projection — collapse + content normalization + language lowercasing — with a normalization profile passed into the id function. That bakes a policy decision (what counts as "the same string") into the lowest-level primitive.

**Decision.**

- Identity is `id_from_atom(atom: &Atom)` -> `AtomId`, a pure function of the atom alone: SHA-256 over a verbatim, length-prefixed serialization of language tag + content. No profile parameter.
- Identity performs no normalization and no collapse. Text is serialized exactly as recorded, so chunking is significant ([Text("a"),Text("b")] ≠ [Text("ab")]). Placeholders still contribute only a type byte, so markup stays out of identity (D16 preserved).
- The language tag is serialized faithfully (`as_str()`), case included — symmetric with text.
- Normalization moves entirely into `crate::normalize` as dumb, explicit, independently-callable primitives. The canonical pipeline is now caller-composed: `id_from_atom(normalize…(atom))`.

**Why.** Keep the lowest primitive as dumb as possible — mechanism, not policy (consistent with faithful Atom construction D17 and minimal LanguageTag validation D19). `id = f(atom)` is referentially transparent where atom_id`(atom, profile)` gave one atom many ids; identity has nothing to decide, so nothing to get wrong. Reaffirms D6 literally — flexibility lives in normalization, and now the hash has zero knobs. Consequence (accepted). An `AtomId` is canonical only across callers who apply the same normalization; the cross-deployment merge guarantee (§10) moves from type-enforced to a shared convention callers must agree on and record. Mitigations: document it loudly, and offer opinionated normalize convenience wrappers (sane defaults composed from the primitives) — deferred.

**Amends D18.** `AtomId = SHA-256(serialize(normalize(collapse(atom))))` no longer holds; serialization is over the raw atom. The TLV/length-prefix framing and in-band version byte survive; the scheme remains fixed-width big-endian length + version byte.

---

### D30 — XLIFF parse: a quick-xml pull-tokenizer returning `Vec<ContentNode>`

**Status:** Accepted &nbsp;|&nbsp; **Enacts D26**

**Decision.**

- `parse_segment(content: &str, mode) -> Result<Vec<ContentNode>, XliffParseError>`. Input is the inline _body_ of a `<source>`/`<target>` (the caller strips the wrapper); output is the raw content tokens. It does **not** build an `Atom`, resolve the language, normalize, or compute identity — the caller composes that pipeline (`Atom::new(lang, nodes)` → `normalize` → `id_from_atom`).
- Implemented with quick-xml's **borrowing pull reader** (`Reader::from_str`) over a small stateful parser. Placeholders are **raw byte-slices of the input**, captured by tracking the reader's byte position — attribute order, quoting, and `<x/>`-vs-`<x></x>` preserved.
- Classification is purely the XLIFF 1.2 content model, by element name: **code-content** `{bpt,ept,ph,it}` → one opaque placeholder (read to element end via `read_to_end`); **text-content** `{g,mrk}` → open/close tags as placeholders, inner text kept; **empty** `{x,bx,ex}` and any self-closed inline → one placeholder. Nesting needs no manual stack: code-content is consumed atomically, so only text-content opens/closes surface as events.
- `XliffParseError` wraps `quick_xml::Error` and decoding uses quick-xml's `unescape`, so quick-xml appears in the adapter's error surface — the same debuggability precedent as D28.

**Why.** The boundary's one job is faithful XLIFF↔node translation; everything beyond "code or text per spec" is interpretation/policy that belongs above the primitive (D1, D26). Returning a bare `Vec<ContentNode>` keeps the seam dumb and leaves Atom assembly, normalization, and identity to the caller (D29).

---

### D31 — Entities: one accumulate strategy, the mode is a single decode-or-not switch

**Status:** Accepted &nbsp;|&nbsp; **Enacts D26's codec**

**Context.** quick-xml 0.40 emits entity references (`&amp;`, `&#10;`) as their own `GeneralRef` events, separate from text — there is no `unescape()` on a `Text` event to lean on.

**Decision.**

- `EntityMode { Logical (default), Verbatim }`, passed to `parse` and symmetrically to `emit`.
- **A single text-building path for both modes:** text runs, entity refs, and CDATA accumulate into a pending `String`, flushed as one `ContentNode::Text` at each element boundary. The mode changes exactly one thing — how a `GeneralRef` is appended: **Logical** decodes it (standard XML entities + numeric char refs, via quick-xml's `unescape`); **Verbatim** keeps its raw bytes.
- An unknown/non-standard entity under Logical is a hard error (`UnknownEntity`); under Verbatim it's kept raw.
- **Entities inside placeholders are never decoded** — code-content is swallowed whole, so its bytes stay raw.

**Why.** One accumulate path (rather than a slice-for-verbatim / accumulate-for-logical fork) makes the mode a one-line difference and far easier to reason about; the small extra allocation is irrelevant at segment size. Decoding stays at the boundary, never in identity (D29). Bonus: accumulation merges adjacent text/entity/CDATA runs into a single node.

---

### D32 — CDATA is character data, handled as text following the mode; other non-inline constructs fail loud

**Status:** Accepted &nbsp;|&nbsp; **Enacts D26 (loud failure); refines D31**

**Context.** A segment body can contain XML constructs beyond text and inline elements. CDATA is the subtle one: XML mixed content (`#PCDATA`) permits a CDATA section _anywhere character data is allowed_, so `<source>a <![CDATA[b]]> c</source>` is well-formed — a CDATA section is just an alternative serialization of character data, not a distinct kind of content.

**Decision.**

- **CDATA is treated as text, following the entity mode** (D31), stored as a `Text` node — not a placeholder, not a new node kind. **Verbatim** keeps the raw `<![CDATA[…]]>` bytes (byte-identical round-trip); **Logical** strips the delimiters and keeps the inner content (its bytes are already literal — CDATA suppresses entity recognition, so logical does _not_ unescape inside it).
- Everything else outside the inline content model is a **hard parse error, never silently dropped**: unknown elements (`UnknownTag`), and comments, processing instructions, XML declarations, doctypes (`UnsupportedConstruct`).

**Why.** The dumb adapter follows the XML spec, where CDATA _is_ character data (text); ascribing it an "opaque, do-not-touch" meaning would be interpretation (D26) and would force a bad model — a placeholder excludes its content from identity (D16), so two different CDATA texts would collide, and a new `ContentNode` variant would leak an escaping concept into the format-free core (D1). Silent dropping of the other constructs is the data loss the fail-loud tenet rejects.

**Consequence (accepted).** Under Logical, a CDATA section is re-serialized to escaped text on emit (`<![CDATA[a&b]]>` → `a&amp;b`) — content-identical, _not_ byte-identical. That is logical mode working as designed (it sees through serialization). A caller needing CDATA to survive byte-intact uses **Verbatim** for that file/segment; the niche "Logical text but byte-faithful CDATA in one segment" is deferred to caller composition (pre-extract the region) and revisited only if a real file demands it — never built on spec. A future strict "error on CDATA" knob remains available if wanted.

---

## Phase Plan (accepted)

Deliberately fine-grained; we reason through each before starting it.

- **Phase 0 — Foundations.** This decision log; crate scaffolding (D10), the testing harness (D9), lint/format setup, and the dependency proposal.
- **Phase 1 — IR + identity, XLIFF-1.2-informed but format-free.**
  - 1a. IR types (the data model; types only, no behavior). ✓ done.
  - 1b.a. Structural collapse — merge adjacent text, drop empty runs (D17, Q3); no dependencies.
  - 1b.b. Canonical serialization (D18) + SHA-256 over it → `AtomId`; adds the `sha2` crate.
  - 1d. Content normalization inserted into the `AtomId` pipeline: NF form, whitespace, case, and BCP-47 via `oxilangtag` (resolves Q4; implements D7, D11).
  - 1e. Invariant tests over the `AtomId` (distinct chunkings → same id; distinct structure → distinct id; collapse / normalize idempotence).
- **Phase 2 — XLIFF 1.2 adapter + fidelity gate.** `parse: XLIFF 1.2 → IR`, `emit: IR → XLIFF 1.2`, round-trip diff test as a CI gate; language-only-tag policy (Q5); `<sub>` extraction (D8).
- **Phase 3+ — Dialects & hard cases.** memoQ / SDL dialects; `<mrk>` (Q2); normalization refinements.

---

## Open Questions

### `<mrk>` handling

The `<sub>` half is resolved (D8). `<mrk>` (annotation spans) is the harder case
and remains open: annotations can wrap arbitrary content and carry semantics
(terms, comments) that may or may not belong in identity. Deferred past Phase 1.
