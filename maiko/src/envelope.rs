use std::{fmt, hash};

use crate::{ActorId, EventId, Meta};

/// The unit carried through all Maiko channels.
///
/// Every event in the system travels as `Arc<Envelope<E>>` - from producer
/// through the broker to each subscriber's mailbox. It pairs the user-defined
/// event payload with [`Meta`] (sender, timestamp, parent ID).
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "E: serde::Serialize",
        deserialize = "E: serde::de::DeserializeOwned"
    ))
)]
pub struct Envelope<E> {
    meta: Meta,
    event: E,
}

impl<E> Envelope<E> {
    /// Create a new envelope tagging the event with the given actor name.
    pub fn new(event: E, actor_id: ActorId) -> Self {
        Self {
            meta: Meta::new(actor_id, None),
            event,
        }
    }

    /// Set the parent event ID for causality tracking.
    ///
    /// Prefer [`Context::send_child_event`](crate::Context::send_child_event) which
    /// sets this automatically. Use `with_parent_id` directly only when constructing
    /// envelopes outside the normal actor context (e.g., in test setup or bridges).
    pub fn with_parent_id(mut self, parent_id: EventId) -> Self {
        self.meta.set_parent(parent_id);
        self
    }

    /// Returns a reference to the event payload.
    ///
    /// This is a convenience method for pattern matching.
    ///
    /// # Example
    ///
    /// ```ignore
    /// match envelope.event() {
    ///     MyEvent::Foo(x) => handle_foo(x),
    ///     MyEvent::Bar => handle_bar(),
    /// }
    /// ```
    #[inline]
    pub fn event(&self) -> &E {
        &self.event
    }

    /// Returns the event metadata (sender, timestamp, parent ID).
    #[inline]
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Shorthand for `self.meta().id()`.
    #[inline]
    pub fn id(&self) -> EventId {
        self.meta.id()
    }
}

impl<E: PartialEq> PartialEq for Envelope<E> {
    fn eq(&self, other: &Self) -> bool {
        self.meta.id() == other.meta.id() && self.event == other.event
    }
}

impl<E: Eq> Eq for Envelope<E> {}

impl<E: hash::Hash> hash::Hash for Envelope<E> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.meta.id().hash(state);
        self.event.hash(state);
    }
}

impl<E: fmt::Debug> fmt::Debug for Envelope<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Envelope")
            .field("id", &self.meta.id())
            .field("sender", &self.meta.actor_name())
            .field("event", &self.event)
            .field("timestamp", &self.meta.timestamp())
            .field("parent_id", &self.meta.parent_id())
            .finish()
    }
}

impl<E: fmt::Display> fmt::Display for Envelope<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Envelope {{ id: {}, sender: {}, event: {} }}",
            self.meta.id(),
            self.meta.actor_name(),
            self.event
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Event;

    #[derive(Clone, Debug)]
    #[allow(unused)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

    #[test]
    fn envelope_debug() {
        let actor = ActorId::new("test-actor");
        let envelope = Envelope::new(TestEvent(42), actor);
        let debug_str = format!("{:?}", envelope);

        assert!(debug_str.contains("TestEvent"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("test-actor"));
    }
}
