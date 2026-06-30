use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Kind(String);

impl Kind {
    pub fn new(label: impl Into<String>) -> Result<Self, KindError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(KindError::Empty);
        }
        Ok(Self(label))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug)]
pub enum KindError {
    Empty,
}
impl fmt::Display for KindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "an observation kind must not be empty"),
        }
    }
}

impl std::error::Error for KindError {}
