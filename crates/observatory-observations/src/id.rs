#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationId([u8; 16]);

impl ObservationId {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }
}
