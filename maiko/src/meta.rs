use std::{fmt, hash, time::SystemTime};

use crate::{ActorId, EventId};

/// Metadata attached to every [`Envelope`](crate::Envelope).
///
/// - `id`: unique event identifier (UUID v4, not monotonic).
/// - `timestamp`: creation time in nanoseconds since Unix epoch (`u64`). Monotonic within a process.
/// - `actor_id`: the actor that emitted the event.
/// - `parent_id`: optional link to a parent event. Set automatically by
///   [`Context::send_child_event`](crate::Context::send_child_event). The test harness
///   uses parent IDs to build event chains (`EventChain`, `ActorTrace`, `EventTrace`).
#[derive(Debug, Clone, PartialEq, Eq, hash::Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Meta {
    id: EventId,
    timestamp: u64,
    actor_id: ActorId,
    parent_id: Option<EventId>,
}

impl Meta {
    /// Construct metadata for a given actor ID and optional parent event ID.
    ///
    /// # Panics
    ///
    /// Panics if the system clock is set before the Unix epoch.
    pub fn new(actor_id: ActorId, parent_id: Option<EventId>) -> Self {
        Self {
            id: EventId::new(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("SystemTime before Unix epoch")
                .as_nanos() as u64,
            actor_id,
            parent_id,
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
        self.actor_id.as_str()
    }

    /// The actor that emitted this event.
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// Optional parent event ID for causality tracking.
    pub fn parent_id(&self) -> Option<EventId> {
        self.parent_id
    }

    pub(crate) fn set_parent(&mut self, parent_id: EventId) {
        self.parent_id = Some(parent_id);
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
        if let Some(parent_id) = self.parent_id() {
            write!(f, ", parent_id: {}", parent_id)?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}
