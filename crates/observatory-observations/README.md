# observatory-observations

Append-only **observations** over [`observatory-core`] atoms.

An atom records only _what a string is_. Everything _about_ a string, or
_between_ strings, is an **observation**: an append-only fact keyed to one or more
`AtomId`s. There are two shapes, told apart only by how many atoms they touch:

- a **property** — a fact about a single atom: reviewed, blacklisted, assigned a
  domain, used in a campaign.
- a **relationship** — a fact among atoms: two are translations of each other,
  one is context for another, several are interchangeable.

## Design

- **Identity is content-derived.** An observation's id is not minted — it is a
  SHA-256 over a canonical serialization of every field (kind, subjects in order,
  both timestamps, and the payload), computed by `id_from_observation`. The same
  observation always yields the same id, so the storage layer dedups
  byte-identical observations naturally — the same integrity contract
  `observatory-core` applies to atoms.
- **Structure, not semantics.** A `Kind` is an open, user-definable label, checked
  only for being non-empty. Which kind is valid at which arity, and what its
  payload must contain, is policy for a layer up.
- **Translation is symmetric.** A translation relationship is a bidirectional
  _equivalence_ between atoms in different languages — `en-US` ⇄ `fr-FR`, with
  neither side privileged as "source." The directional source→target view a
  classic TM bakes into storage is instead _derived_ at export time, by choosing
  which language is the source for the file you are producing. Subject order is
  preserved as given and feeds the id, but the crate ascribes it no meaning;
  whether it is a direction or noise is the kind's semantics.
- **Canonicalization is the caller's.** The crate records every observation
  exactly as given. Canonicalizing a symmetric kind — sorting its subjects so the
  two orders collapse to one id — is the caller's job, applied before calling
  `id_from_observation`, just as `observatory-core` records atoms faithfully and
  leaves normalization to the caller.
- **Bitemporal time.** `recorded_at` is when the fact was written; `effective_at`
  is when it became true. They differ only when history is backfilled, so
  `effective_at` defaults to `recorded_at`. The clock is the caller's — timestamps
  are arguments, never read from an ambient source.
- **The payload is a kitchen sink.** Everything kind-specific — provenance,
  scores, reasons — lives in a `serde_json::Value` payload. The envelope (kind,
  subjects, the two timestamps) is all every observation shares; the id is a
  derived projection over the whole, not a field.

## Example

```rust
use std::time::SystemTime;

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_observations::{Kind, Observation};
use serde_json::json;

// Two atoms whose ids we have already computed.
let en = Atom::new(
    LanguageTag::from_string("en-US").unwrap(),
    [ContentNode::text("Hello")],
);
let fr = Atom::new(
    LanguageTag::from_string("fr-FR").unwrap(),
    [ContentNode::text("Bonjour")],
);
let (en_id, fr_id) = (id_from_atom(&en), id_from_atom(&fr));

// The caller supplies the clock; the observation id is content-derived.
let now = SystemTime::now();

// "Bonjour" and "Hello" are translations of each other — a symmetric
// equivalence, not a directed source→target row.
let translation = Observation::relationship(
    Kind::new("translation").unwrap(),
    vec![fr_id, en_id],
    now,
    None, // effective_at defaults to recorded_at
    json!({ "author": "deepl:v2", "confidence": 0.91 }),
);

assert_eq!(translation.subjects(), &[fr_id, en_id]);
assert_eq!(translation.effective_at(), translation.recorded_at());
```

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at
your option.

[`observatory-core`]: ../observatory-core/
