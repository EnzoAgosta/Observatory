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

- **Faithful, intent-named constructors.** `Observation::property` and
  `Observation::relationship` record exactly the subjects they are given and judge
  nothing about them — the way `Atom::new` faithfully records its content nodes.
  The two names are a hint to the reader, not a type-enforced split: both build
  the same shape, and nothing stops a one-subject relationship. Sharper
  constructors (a strictly binary one, say) or deeper validation are conveniences
  to add on top later, not policy the primitive imposes now.
- **Structure, not semantics.** A `Kind` is an open, user-definable label, checked
  only for being non-empty. Which kind is valid at which arity, and what its
  payload must contain, is policy for a layer up.
- **Translation is symmetric.** A translation relationship is a bidirectional
  _equivalence_ between atoms in different languages — `en-US` ⇄ `fr-FR`, with
  neither side privileged as "source." The directional source→target view a
  classic TM bakes into storage is instead _derived_ at export time, by choosing
  which language is the source for the file you are producing. Subject order is
  preserved as given but ascribed no meaning by the crate; whether it is a
  direction or noise is the kind's semantics.
- **Recording is faithful; dedup is the caller's.** The crate stores every
  observation exactly as given and collapses nothing — two recordings that differ
  only in subject order are distinct observations, and a re-asserted fact is a new
  event. Canonicalizing symmetric relationships, deduplicating, and collapsing the
  log into read models is the job of the layer that stores them (the store / app),
  exactly as `observatory-core` records atoms faithfully and leaves
  dedup-by-`AtomId` to the caller.
- **Bitemporal time.** `recorded_at` is when the fact was written; `effective_at`
  is when it became true. They differ only when history is backfilled, so
  `effective_at` defaults to `recorded_at`. The clock is the caller's — timestamps
  are arguments, never read from an ambient source.
- **The payload is a kitchen sink.** Everything kind-specific — provenance,
  scores, reasons — lives in a `serde_json::Value` payload. The envelope (id,
  kind, subjects, the two timestamps) is all every observation shares.

## Example

```rust
use std::time::SystemTime;

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_observations::{Kind, Observation, ObservationId};
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

// The caller mints the event id and supplies the clock.
let obs_id = ObservationId::from_bytes([0; 16]);
let now = SystemTime::now();

// "Bonjour" and "Hello" are translations of each other — a symmetric
// equivalence, not a directed source→target row.
let translation = Observation::relationship(
    obs_id,
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
