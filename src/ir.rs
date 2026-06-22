//! The intermediate representation (IR): the canonical data model for a
//! translation segment, distilled from XLIFF 1.2 but independent of it (D3).
//!
//! A segment is a sequence of content nodes (text and positional tag
//! placeholders). Tag placeholders carry *kind* (open / close / standalone) and
//! *pairing*, but not payload (D2).
//!
//! Types only for now; behavior arrives in later sub-phases (see
//! `docs/DECISIONS.md`, Phase 1a–1e).
