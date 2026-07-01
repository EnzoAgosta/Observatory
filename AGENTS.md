# AGENTS.md

A lakehouse for translation data: translation segments become content-addressed
atoms, and every fact about them is recorded as an append-only observation —
event sourcing applied to translation. See [`README.md`](README.md) for the full
context and design.

Start every session by reading [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md) — it
lays out the tenets this repo is built on and the heuristic for deciding where a
change belongs. Each crate has its own `README.md` and may have its own
`DESIGN.md`; read them when editing code in that crate or looking for
crate-level context.

This is a Cargo workspace (edition 2024) under `crates/`. Use the typical cargo
commands to interact with it.

Documentation in this repo is deliberate and prose-heavy — read it before
writing, keep it in the same voice, and document new code with the same care.
