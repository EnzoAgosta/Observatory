# observatory-store — design

The backend-agnostic trait crate for Observatory persistence: it defines the
contracts a concrete store implements, and nothing else.

## Role, and the line it does not cross

`observatory-store` defines two traits — [`AtomStore`] and
[`ObservationStore`] — plus a single [`StoreError`]. The traits speak only
domain types (`Atom` / `AtomId` / `Observation` / `ObservationId` / `Kind`)
and the error; they never name a backend's concrete types. A concrete
implementation (today: [`observatory-lance`]) supplies the trait; the rest of
the system programs against the trait.

This is the seam: code that needs a real store depends on the concrete
implementation; code that needs to be backend-agnostic depends on the traits
in this crate.

## Why a trait crate, and why now

Distilling the traits only after both concrete stores (`AtomStore` and
`ObservationStore` over Lance in [`observatory-lance`]) existed is deliberate
— the traits are _derived, not predicted_ (a corollary of tenet 7 in
[`PHILOSOPHY.md`](../../docs/PHILOSOPHY.md)). The two concrete stores shared
an obvious shape once both existed; the traits crystallize that shape. The
split leaves `observatory-store` free of Lance (or any other backend) as a
dependency — letting downstream code, including test doubles, depend on the
contract without dragging the backend's dependency graph in.

## Write methods are streaming-first

`put_atoms` and `put_observations` take a `Stream<Item = T>` rather than a
`&[T]`. The motivating workload is feeding atoms or observations produced
incrementally — a parser over a large XLIFF, a network feed — without
materializing the whole batch in memory. A concrete implementation is free
to buffer the stream into chunks of a size it picks; chunking policy is the
implementation's concern, never the trait's. Callers with an in-memory batch
use `futures::stream::iter`.

The stream bound is `Stream + Unpin + Send + 'static`. `'static` because the
future returned by an `async_trait` method is owned and may outlive the
call's borrow scope; `Send` because the future is `Send` under `async_trait`
and the stream it polls must be too; `Unpin` so the implementation can poll
it without `Pin` ceremony.

Tradeoff accepted: this method shape is generic, so the traits are **not
object-safe** — `dyn AtomStore` does not work. Callers hold `impl AtomStore`
(generic) or the concrete type. We accept that: stores are constructed at
the call site, and the concrete type is known there. A box-friendly variant
(taking `Pin<Box<dyn Stream + Send>>`) can be added if a real `dyn` use
arises.

## Reads return `Vec<T>`

Reads materialize their result. The latency budget is generous (storage
dwarfs the LLM calls downstream), and a single materialized vector is the
simplest correct shape. A streaming read API can be added _alongside_ the
vec-returning methods later if a real need emerges — it would not replace
them.

## What's off the trait

Lifecycle (`open`, `create`) and backend-specific maintenance
(`ensure_indexes`, `optimize_indexes`, `compact`, `cleanup_versions`) stay
**off the traits**. They take backend-specific options and have no
domain-level reading: `ensure_indexes` on Lance builds named BTREE/BITMAP
indexes over named columns; on a hypothetical other backend it might build
something else entirely, or be a no-op. They live as inherent methods on the
concrete implementation — code that needs them holds the concrete type, not
the trait. A backend that has no maintenance primitives simply omits them.

## Error strategy

A single `StoreError` carries the variants every backend shares (a corrupt
row at the read-back trust boundary, an invalid domain type encountered in
stored bytes) plus a single `Backend { detail, source }` escape hatch. The
backend maps its native errors into `Backend` at the trait boundary; the
trait never names a backend's error type. `From` impls exist for the
domain-level errors (`LanguageTagError`, `KindError`, `serde_json::Error`)
so decodersconvert naturally.

[`AtomStore`]: crate::AtomStore
[`ObservationStore`]: crate::ObservationStore
[`StoreError`]: crate::StoreError
[`observatory-lance`]: ../observatory-lance/
