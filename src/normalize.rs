//! Normalization: the configurable layer beneath identity (D6).
//!
//! Named profiles decide *how* Unicode content and BCP-47 language tags are
//! normalized before hashing; the identity formula itself stays fixed. The
//! default profile lowercases tags, requires a region, and does not fold region
//! variants (D7).
//!
//! The profile interface and the canonical default are Phase 1d (open question
//! Q4, implementing D7).
