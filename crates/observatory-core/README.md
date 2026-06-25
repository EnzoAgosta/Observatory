# observatory-core

The normalization core of [Observatory]. It turns translation segments into content-addressed **atoms**.

A translation segment — text that may carry inline formatting — is recorded as an **atom**: a single-language string in which every inline tag is reduced to an opaque _placeholder_.
Each atom has a content-derived identity, the **`AtomId`**: a SHA-256 over a canonical serialization of the atom _exactly as recorded_.
The same atom always produces the same `AtomId`; identity is deliberately dumb and applies no normalization itself, excluding only placeholder _markup_ (so the same text tagged two different ways still shares an `AtomId`).

The point is to decouple a string from its relationships. An atom records only _what a string is_; how strings relate — translations, reviews, and other facts — is expressed separately as observations over atoms, never baked into the atom itself.

## Core concepts

- **`Atom`** — a single-language string as an ordered list of content nodes.
  It is recorded faithfully: an atom stores exactly what it was given, and the original string is recovered by joining its nodes back together.
- **Content node** — either translatable _text_ or an opaque _placeholder_ standing in for non-text (an inline tag, a variable, a code).
  A placeholder's markup is preserved for reconstruction but never interpreted, and it does not affect identity beyond its presence and position.
- **`LanguageTag`** — a validated BCP-47 tag. It must be well-formed and include a region; script and other subtags are optional.
  Validation is structural only (no registry lookup), so private-use tags are accepted.
- **`normalize`** — a toolkit of explicit, composable primitives (collapse, trim, Unicode form, language case) for canonicalizing content _before_ you take its id.
  Nothing is applied automatically; you compose the steps you want.
- **`AtomId`** — the content-addressed identity.

## Identity

```text
AtomId = SHA-256( serialize( atom ) )
```

The serialization is an unambiguous, length-prefixed byte layout of the atom as recorded: the language tag verbatim, then each content node — text runs by their bytes, placeholders by a single marker (their markup is excluded).
It is _not_ normalized, so text, chunking, and language case are all significant.

To make "the same" content across different taggings or casings share an id, canonicalize the atom first with the `normalize` primitives, then hash.
Keeping the id itself dumb makes it a pure function of the atom and leaves all canonicalization policy with the caller.

## Example

```rust
use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_core::normalize::{CollapseMode, collapse};

let tag = LanguageTag::from_string("en-US").unwrap();

// The same text recorded as one run vs. two. Identity is faithful, so as-is
// these are different atoms with different ids.
let merged = Atom::new(tag.clone(), [ContentNode::text("Hello, world")]);
let split = Atom::new(
    tag.clone(),
    [ContentNode::text("Hello, "), ContentNode::text("world")],
);
assert_ne!(id_from_atom(&merged), id_from_atom(&split));

// Normalize first — collapsing adjacent text — and they share an id.
let canonical = Atom::new(tag, collapse(split.content(), CollapseMode::CollapseAdjacent));
assert_eq!(id_from_atom(&merged), id_from_atom(&canonical));
```

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.

[Observatory]: ../../README.md
