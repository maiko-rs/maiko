use std::{hash::Hash, sync::Arc};

/// Unique identifier for a registered actor.
///
/// Returned by [`Supervisor::add_actor`](crate::Supervisor::add_actor). Use `ActorId` to:
///
/// - Identify actors in event metadata (sender information)
/// - Reference actors in test harness assertions
/// - Track event flow between specific actors
///
/// `ActorId` is cheap to clone and safe to serialize. Equality works correctly
/// across serialization boundaries (uses string comparison with a fast-path
/// for pointer equality when IDs share the same allocation).
///
/// # Example
///
/// ```ignore
/// let producer = sup.add_actor("producer", |ctx| Producer::new(ctx), &[DefaultTopic])?;
/// let consumer = sup.add_actor("consumer", |ctx| Consumer::new(ctx), &[DefaultTopic])?;
///
/// // Use in test harness
/// let mut test = Harness::new(&mut sup).await;
/// test.send_as(&producer, MyEvent::Data(42)).await?;
/// assert!(test.actor(&consumer).events_received() > 0);
///
/// // Access sender from event metadata
/// let sender: &ActorId = envelope.meta().actor_id();
/// ```
///
/// ### Creating IDs for External Systems
///
/// For bridge actors that receive events from external sources (IPC, WebSocket),
/// use `ActorId::new()` to create identifiers for those sources.
#[derive(Debug, Clone, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorId(Arc<str>);

impl ActorId {
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }

    /// Returns the string representation of this actor ID.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for ActorId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for ActorId {}

impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Hash for ActorId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl From<&str> for ActorId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for ActorId {
    fn from(s: String) -> Self {
        Self(Arc::from(s))
    }
}
