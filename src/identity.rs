//! Content-addressed identity for atoms.
//!
//! `identity = SHA-256(normalized content ‖ canonical BCP-47 language tag)`
//! (D4, D5). Nothing else is fed to the hash: direction, domain, provenance and
//! the like are observations, not identity.
//!
//! The exact serialization fed to the hash is Phase 1b (open question Q3).
