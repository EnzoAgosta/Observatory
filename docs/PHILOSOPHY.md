# Observatory — Philosophy

How to think about this repository. If you are new here — human or AI — read this
before changing code. The [decision log](DECISIONS.md) records _what_ we decided
and _why_, decision by decision; this document distills the _mindset_ those
decisions all flow from, so you can make new choices that fit.

## The one idea

A string and its relationships are different things, and conflating them is the
mistake the whole system is built to avoid. Observatory's job is to give a string
a **stable, content-derived identity** — and nothing else. What a string _means_,
how it _relates_ to other strings (translation, review, provenance, domain), how
it should be _validated_ or _normalized_ for a given use — all of that is someone
else's job, expressed as **observations** layered on top, never baked into the
string's identity.

Everything below is a corollary of taking that seriously.

## Tenets

### 1. Dumb by default — mechanism, not policy

The lowest layers do the least thinking possible. An `Atom` is a faithful
recording of "what is text and what is a placeholder," in order — it does not
validate, interpret, or rewrite what it is given (D14, D16). `id_from_atom` is a
pure function from an atom to bytes to a hash — it normalizes nothing, decides
nothing (D29). `LanguageTag` checks only that a tag is well-formed and has a
region; it does not consult a registry or judge whether a locale is "real" (D7,
D11, D19). The XLIFF adapter is a stateless codec driven purely by the spec's
content model — it never interprets ids, pairing, or meaning (D26).

A primitive that has nothing to decide has nothing to get wrong. The surface for
surprising behavior shrinks to "is the transform faithful and deterministic."
_Policy_ — what counts as valid, what counts as "the same," what relates to what
— lives above, where the context to decide it exists.

**Dumb means no policy, not no rigor.** The serialization is still injective and
platform-independent; the hash is still pinned and reproducible. Dumb ≠ sloppy.

### 2. Identity records what a string _is_; relationships are observations

Feed the identity function only what makes two byte-identical strings _genuinely
different atoms_ (D5). Language passes that test — `"Gift"` in English and German
are different translatable objects. Direction (source vs. target), domain, client,
register, recency, review status — all fail it; they are facts _about_ an atom,
recorded as append-only observations elsewhere, not constitutive _of_ it (D1, D5,
D22).

The litmus question for anything you're tempted to put "on" the atom: _would two
strings with identical bytes and language ever need different identities because
of this?_ If no, it's an observation a layer up.

### 3. Faithful and reversible: record first, derive later

Construction never mutates. `Atom::new` stores exactly the nodes it was given —
adjacent runs, empty runs, original case — all preserved (D17). Reconstruction is
a blind, lossless join of every node's data (D16). Placeholders hold the _raw_
original markup as opaque bytes and are never parsed (D16, D26).

Identity is then a _derived projection_ over that faithful recording — a
materialized view, not the source of truth. This is why structural equality
(`==`) and identity (`AtomId`) are deliberately different relations: two atoms can
share an `AtomId` without being `==` (they differ only in placeholder markup).
**Always dedup and compare by `AtomId`, never by `==`.**

### 4. Identity is pure; normalization is the caller's, and explicit

`id_from_atom(atom)` takes nothing but the atom. The same atom always yields the
same id — referentially transparent (D29). It does not collapse chunking, fold
whitespace, or lowercase the language; text, chunking, and language case are all
significant, byte-for-byte.

Canonicalization — the thing that makes "the same" content across different
taggings or casings collapse to one id — is a **separate toolkit of small,
idempotent, composable primitives** in `normalize`, applied _by the caller_ before
hashing: `id_from_atom(normalize…(atom))`. Nothing is applied automatically; you
compose exactly the steps you want, and they are visible at the call site.
Convenience "sane default" bundles may exist, but only ever as thin compositions
of the public primitives — never hidden magic.

This is the move from `id = f(atom, profile)` (a hidden policy knob) to
`id = f(atom)` (pure) plus an explicit `normalize(atom)` transform. The hash has
zero knobs.

### 5. The burden is on the caller — on purpose

This is a low-level primitive consumed by other code (adapters, storage, tooling),
not an application. So it pushes responsibility _outward_: the caller validates
the document, decides which nodes become atoms, supplies the language tag, chooses
the normalization, and records the relationships. The crate gives sharp, total,
predictable building blocks and trusts the caller to assemble them.

The trade is real and chosen: an `AtomId` is only canonical across callers who
normalize _identically_. That guarantee moves from type-enforced to a shared,
documented convention (D29). We accept that because the audience is deliberate
callers who want the lever, not end users who want hand-holding.

### 6. Reproducibility is the bet

A content-addressed id is only valuable if it is **globally reproducible**: the
same string yields the same id in every deployment, so corpora are shareable and a
canonical public id exists (D4, D6). That is the entire reason for the choices
that look conservative — SHA-256 because it reproduces in one line in any language
(D4); a fixed serialization with an in-band version byte so layouts can never
silently collide (D18); an explicit edge-trim set and a pinned Unicode version so
no _ambient_ library behavior leaks into the bytes (D21); a committed lockfile so
our own builds can't drift (D23).

Anything that feeds the hash must be pinned by the spec, not by the environment.
When in doubt, make it explicit and deterministic.

### 7. Don't build on spec; earn every abstraction

Build the dumbest thing that is also _correct_, and let real needs — not imagined
ones — drive complexity. We deferred the placeholder payload's shape until real
XLIFF demanded it (D15→D16), stayed a single crate until the adapter justified a
workspace (D10→D24), and keep the adapter a two-function codec until a real file
proves it must be smarter (D26). Flexibility that isn't needed yet is a footgun,
not a feature (D6).

If you're adding a layer, base class, mode, or config knob "for later," stop. Add
it when "later" arrives with a concrete case.

### 8. Fail loud

When an input can't be honored, error — never guess. A language tag without a
region is rejected, not defaulted (D7). An unknown XML entity is a parse error,
not a silent pass-through (D26). Guessing manufactures false data that is worse
than a clean failure, especially in a system whose whole value is trustworthy
identity.

## Where does my change belong? (the heuristic)

Before adding behavior here, ask:

- Does it require **interpreting** what content _means_ (tag roles, pairing,
  semantics)? → A layer above. Here, content is text-or-opaque-bytes, nothing
  more.
- Does it encode a **policy** (what's valid, what's "the same," which locales are
  real)? → A layer above, or a caller-chosen `normalize` step. The identity core
  holds no policy.
- Does it record a **relationship** between strings (translation, review,
  "sub-of," provenance)? → An observation, never identity.
- Is it a **faithful, total, deterministic** transform of bytes that every caller
  would want identically? → It may belong here. Make it pure, idempotent, and
  explicit.

If a feature feels like a special case _on the atom_, it is almost always an
observation one layer up. That single instinct keeps you aligned with everything
in the decision log.

## Vocabulary

- **Atom** — a single-language string recorded as an ordered list of content
  nodes; the content-addressed unit (D13).
- **AtomId** — the SHA-256 identity derived from an atom (D4, D29).
- **ContentNode** — one run of an atom: `Text` (translatable) or `Placeholder`
  (opaque non-text markup) (D16, D27).
- **Placeholder** — opaque raw markup standing in for an inline tag, code, or
  variable; identity counts its position and presence, never its bytes (D16).
- **Observation** — an append-only fact _about_ an atom or _between_ atoms
  (translation, review, provenance); lives outside this crate (D1, D22).
- **Normalization** — caller-applied, composable transforms that canonicalize
  content before hashing; never automatic (D29).

## How we evolve

Decisions are recorded in [`DECISIONS.md`](DECISIONS.md), **append-only**: a
choice is never edited once accepted; when we change our minds we add a
superseding decision and mark the old one. Read the log to understand _why_
something is the way it is before changing it — most "why is this so minimal?"
questions are answered there, deliberately.
