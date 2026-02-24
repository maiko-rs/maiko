use std::{fmt, hash};
use uuid::Uuid;

/// Unique identifier for an event, backed by a UUID v4.
///
/// Every [`Envelope`](crate::Envelope) is assigned an `EventId` at creation time.
/// IDs are random (not monotonic) - use [`Meta::timestamp()`](crate::Meta::timestamp)
/// for ordering.
///
/// `EventId` is `Copy` and cheap to pass around. Use [`Display`](std::fmt::Display)
/// to get the UUID string representation, or [`value()`](Self::value) for the raw `u128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, hash::Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
pub struct EventId(u128);

impl EventId {
    /// Generate a new random event ID (UUID v4).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().as_u128())
    }

    /// Returns the raw `u128` value.
    pub fn value(&self) -> u128 {
        self.0
    }

    /// Returns the ID as a [`Uuid`].
    pub fn to_uuid(&self) -> Uuid {
        Uuid::from_u128(self.0)
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
        write!(f, "{}", self.to_uuid())
    }
}

impl Default for EventId {
    fn default() -> Self {
        EventId::new()
    }
}
