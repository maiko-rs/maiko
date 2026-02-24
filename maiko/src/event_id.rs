use std::{fmt, hash};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, hash::Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EventId(u128);

impl EventId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().as_u128())
    }

    pub fn value(&self) -> u128 {
        self.0
    }
}

impl From<u128> for EventId {
    fn from(value: u128) -> Self {
        EventId(value)
    }
}

impl From<EventId> for u128 {
    fn from(value: EventId) -> Self {
        value.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Uuid::from_u128(self.0))
    }
}

impl Default for EventId {
    fn default() -> Self {
        EventId::new()
    }
}
