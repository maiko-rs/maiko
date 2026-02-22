use std::{fmt, hash, time::SystemTime};

use uuid::Uuid;

use crate::{ActorId, EventId};

/// Metadata attached to every [`Envelope`](crate::Envelope).
///
/// - `id`: unique event identifier (UUID v4, not monotonic).
/// - `timestamp`: creation time in nanoseconds since Unix epoch (`u64`). Monotonic within a process.
/// - `actor_id`: the actor that emitted the event.
/// - `correlation_id`: optional link to a parent event. Set automatically by
///   [`Context::send_child_event`](crate::Context::send_child_event). The test harness
///   uses correlation to build event chains (`EventChain`, `ActorTrace`, `EventTrace`).
#[derive(Debug, Clone, PartialEq, Eq, hash::Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Meta {
    id: EventId,
    timestamp: u64,
    actor_id: ActorId,
    correlation_id: Option<EventId>,
}

impl Meta {
    /// Construct metadata for a given actor name and optional correlation id.
    ///
    /// # Panics
    ///
    /// Panics if the system clock is set before the Unix epoch.
    pub fn new(actor_id: ActorId, correlation_id: Option<EventId>) -> Self {
        Self {
            id: Uuid::new_v4().as_u128(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("SystemTime before Unix epoch")
                .as_nanos() as u64,
            actor_id,
            correlation_id,
        }
    }

    /// Unique identifier for this envelope.
    pub fn id(&self) -> EventId {
        self.id
    }

    /// Timestamp in nanoseconds since Unix epoch (u64 truncation).
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Name of actor that sent the event.
    pub fn actor_name(&self) -> &str {
        self.actor_id.name()
    }

    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// Optional value of correlation data.
    /// It might by a parent event id, but it's up to the user to define its meaning.
    pub fn correlation_id(&self) -> Option<EventId> {
        self.correlation_id
    }
}

impl fmt::Display for Meta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Event Meta {{ id: {}, timestamp: {}, actor_name: {}",
            self.id(),
            self.timestamp(),
            self.actor_name(),
        )?;
        if let Some(correlation_id) = self.correlation_id() {
            write!(f, ", correlation_id: {}", correlation_id)?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}
