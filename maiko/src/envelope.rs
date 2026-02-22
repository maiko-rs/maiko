use std::{fmt, hash, ops};

use crate::{ActorId, Event, EventId, Meta};

/// The unit carried through all Maiko channels.
///
/// Every event in the system travels as `Arc<Envelope<E>>` - from producer
/// through the broker to each subscriber's mailbox. It pairs the user-defined
/// event payload with [`Meta`] (sender, timestamp, correlation ID).
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

    /// Create a new envelope with an explicit correlation id.
    ///
    /// Use this to link child events to a parent or to group related flows.
    pub fn with_correlation(event: E, actor_id: ActorId, correlation_id: EventId) -> Self {
        Self {
            meta: Meta::new(actor_id, Some(correlation_id)),
            event,
        }
    }

    /// Returns a reference to the event payload.
    ///
    /// This is a convenience method for pattern matching. For method calls,
    /// you can also use `Deref` (e.g., `envelope.some_event_method()`).
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

    #[inline]
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    #[inline]
    pub fn id(&self) -> EventId {
        self.meta.id()
    }
}

impl<E: Event> From<(&E, &Meta)> for Envelope<E> {
    fn from((event, meta): (&E, &Meta)) -> Self {
        Envelope::<E> {
            meta: meta.clone(),
            event: event.clone(),
        }
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

// TODO remove in 0.3.0
// NOTE: This Deref impl is scheduled for removal in 0.3.0.
// Prefer `envelope.event()` over `*envelope` or direct field access.
impl<E: Event> ops::Deref for Envelope<E> {
    type Target = E;
    fn deref(&self) -> &E {
        &self.event
    }
}

impl<E: fmt::Debug> fmt::Debug for Envelope<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Envelope")
            .field("id", &self.meta.id())
            .field("sender", &self.meta.actor_name())
            .field("event", &self.event)
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
    use std::sync::Arc;

    use super::*;

    #[derive(Clone, Debug)]
    #[allow(unused)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

    #[test]
    fn envelope_debug() {
        let actor = ActorId::new(Arc::from("test-actor"));
        let envelope = Envelope::new(TestEvent(42), actor);
        let debug_str = format!("{:?}", envelope);

        assert!(debug_str.contains("TestEvent"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("test-actor"));
    }
}
