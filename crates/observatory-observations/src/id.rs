//! The identity of an observation.

/// A unique identifier for one observation *event*.
///
/// Unlike an [`AtomId`](observatory_core::identity::AtomId) — which is *derived*
/// from an atom's content, so the same atom always hashes to the same id — an
/// `ObservationId` is *minted*: it identifies an occurrence, not a value. Two
/// observations can carry byte-identical facts (the same atom approved by two
/// reviewers, or one fact re-asserted later) and must remain distinct events, so
/// their ids are independent rather than content-derived.
///
/// The 16 bytes are an opaque identifier — a ULID or UUID, say. Minting one needs
/// a clock and entropy, ambient state this crate deliberately does not touch: the
/// caller supplies the bytes, exactly as it supplies an observation's timestamps.
/// And as with `AtomId`, the *text* encoding of those bytes (hex, base32, …) is a
/// presentation concern for the caller, so only [`from_bytes`](Self::from_bytes)
/// and [`bytes`](Self::bytes) are offered here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationId([u8; 16]);

impl ObservationId {
    /// Wraps 16 raw bytes as an `ObservationId` — e.g. a ULID minted by the
    /// caller, or one read back from storage.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The raw 16 bytes.
    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_bytes() {
        let raw = [7u8; 16];
        assert_eq!(ObservationId::from_bytes(raw).bytes(), raw);
    }

    #[test]
    fn distinct_bytes_are_distinct_ids() {
        assert_ne!(
            ObservationId::from_bytes([0u8; 16]),
            ObservationId::from_bytes([1u8; 16]),
        );
    }
}
