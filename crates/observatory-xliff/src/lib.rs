//! XLIFF 1.2 adapter for [`observatory`]: parsing XLIFF 1.2 segments into
//! [`observatory::ir::Atom`]s and emitting atoms back to XLIFF 1.2.
//!
//! This is a Phase 2 skeleton — the parse/emit implementation and the
//! round-trip fidelity gate (D9, D3) land here once Phase 1 is settled.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
