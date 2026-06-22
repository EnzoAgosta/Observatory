# Observatory

Observatory turns translation segments into content-addressed **atoms**.

A translation segment — text that may carry inline formatting — is recorded as an
**atom**: a single-language string in which every inline tag is reduced to an
opaque *placeholder*. Each atom has a stable, content-derived identity, the
**`AtomId`**: a SHA-256 over a canonical, normalized serialization of its content
and language. Identical content in the same language always produces the same
`AtomId`, no matter how the segment was tagged or how its text was split into
runs.

The point is to decouple a string from its relationships. An atom records only
*what a string is*; how strings relate — translations, reviews, and other facts —
is expressed separately as observations over atoms, never baked into the atom
itself.

## Core concepts

- **`Atom`** — a single-language string as an ordered list of content nodes. It is
  recorded faithfully: an atom stores exactly what it was given, and the original
  string is recovered by joining its nodes back together.
- **Content node** — either translatable *text* or an opaque *placeholder*
  standing in for non-text (an inline tag, a variable, a code). A placeholder's
  markup is preserved for reconstruction but never interpreted, and it does not
  affect identity beyond its presence and position.
- **`LanguageTag`** — a validated BCP-47 tag. It must be well-formed and include a
  region; script and other subtags are optional. Validation is structural only
  (no registry lookup), so private-use tags are accepted.
- **`NormalizationProfile`** — the explicit, conservative knobs deciding how
  content is canonicalized before hashing (Unicode form, edge whitespace).
- **`AtomId`** — the content-addressed identity.

## Identity, in one line

```text
AtomId = SHA-256( serialize( normalize( collapse( atom ) ) ) )
```

1. **collapse** — merge adjacent text runs, drop empty ones; leave placeholders alone.
2. **normalize** — apply the chosen `NormalizationProfile`, and lowercase the language tag.
3. **serialize** — an unambiguous, length-prefixed byte layout.
4. **hash** — SHA-256.

Because identity is a normalized projection, two atoms can be structurally
different yet share an `AtomId`. Always compare and key by `AtomId`, never by
structural equality.

## Example

```rust
use observatory::ir::{Atom, ContentNode, LanguageTag};
use observatory::identity::atom_id;
use observatory::normalize::NormalizationProfile;

let tag = LanguageTag::parse("en-US").unwrap();
let atom = Atom::new(tag, [
    ContentNode::text("Click "),
    ContentNode::placeholder("<b>"),
    ContentNode::text("here"),
    ContentNode::placeholder("</b>"),
]);

let id = atom_id(&atom, &NormalizationProfile::default());
println!("{id}");
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
