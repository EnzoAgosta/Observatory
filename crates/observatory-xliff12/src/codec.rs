//! How the adapter treats XML entities when crossing the boundary.
//!
//! A [`Codec`] is the single configuration both [`parse`](crate::parse) and
//! [`emit`](crate::emit) take. It is deliberately tiny — its only knob is whether
//! text is carried as logical Unicode or as raw, escaped bytes — and it must be
//! the *same* on both sides of a round-trip: the mode is not recorded on the
//! atom, so passing one codec object to both calls is what keeps parse and emit
//! symmetric.

/// How XML entities in *text* are treated. Placeholder markup is always raw and
/// is unaffected by this choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityMode {
    /// Text is XML-unescaped to its Unicode form on parse and re-escaped on emit.
    /// Round-trip is *content-identical*: two fragments that differ only in how a
    /// character was escaped (`&amp;`, `&#38;`, `&#x26;`) collapse to the same
    /// atom — which is what makes the `AtomId` reproducible across encodings.
    Logical,
    /// Text is kept exactly as written, entities and all. Round-trip is
    /// *byte-identical*, but identity then hashes the escaped form — a deliberate
    /// caller choice, not the default.
    Verbatim,
}

/// The boundary configuration shared by [`parse`](crate::parse) and
/// [`emit`](crate::emit).
///
/// Use the same `Codec` for both halves of a round-trip. [`Codec::default`] is
/// [`EntityMode::Logical`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codec {
    /// How text entities are treated; see [`EntityMode`].
    pub entities: EntityMode,
}

impl Codec {
    /// A codec that carries text as logical Unicode (the default).
    pub fn logical() -> Self {
        Self {
            entities: EntityMode::Logical,
        }
    }

    /// A codec that carries text verbatim, preserving the original escaping.
    pub fn verbatim() -> Self {
        Self {
            entities: EntityMode::Verbatim,
        }
    }
}

impl Default for Codec {
    fn default() -> Self {
        Self::logical()
    }
}
