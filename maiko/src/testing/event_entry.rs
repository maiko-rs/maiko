use std::{hash, sync::Arc};

use crate::{ActorId, Envelope, Event, EventId, Meta, Topic};

/// A record of an event delivery from one actor to another.
///
/// Each `EventEntry` represents a single delivery: the same event sent to
/// multiple actors produces multiple entries (one per recipient).
///
/// # Fields
///
/// - `event`: The envelope containing the event payload and metadata
/// - `topic`: The topic under which this event was routed
/// - `actor_id`: The receiving actor
#[derive(Debug, Clone, PartialEq, Eq, hash::Hash)]
pub struct EventEntry<E: Event, T: Topic<E>> {
    pub(crate) event: Arc<Envelope<E>>,
    pub(crate) topic: Arc<T>,
    pub(crate) actor_id: ActorId,
}

impl<E: Event, T: Topic<E>> EventEntry<E, T> {
    pub(crate) fn new(event: Arc<Envelope<E>>, topic: Arc<T>, actor_id: ActorId) -> Self {
        Self {
            event,
            topic,
            actor_id,
        }
    }

    /// Returns the unique ID of this event.
    #[inline]
    pub fn id(&self) -> EventId {
        self.event.id()
    }

    /// Returns a reference to the event payload.
    #[inline]
    pub fn payload(&self) -> &E {
        self.event.event()
    }

    /// Returns the event metadata (sender, timestamp, parent ID).
    #[inline]
    pub fn meta(&self) -> &Meta {
        self.event.meta()
    }

    /// Returns the topic this event was routed under.
    #[inline]
    pub fn topic(&self) -> &T {
        &self.topic
    }

    /// Returns the name of the actor that sent this event.
    #[inline]
    pub fn sender(&self) -> &ActorId {
        self.meta().actor_id()
    }

    /// Returns the name of the actor that received this event.
    #[inline]
    pub fn receiver(&self) -> &ActorId {
        &self.actor_id
    }

    /// Returns true if this event was received by the specified actor.
    #[inline]
    pub(crate) fn receiver_actor_eq(&self, actor_id: &ActorId) -> bool {
        self.actor_id == *actor_id
    }

    /// Returns true if this event was sent by the specified actor.
    #[inline]
    pub(crate) fn sender_actor_eq(&self, actor_id: &ActorId) -> bool {
        self.meta().actor_id() == actor_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultTopic;

    #[derive(Clone, Debug)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

    fn make_entry() -> (EventEntry<TestEvent, DefaultTopic>, ActorId, ActorId) {
        let sender_id = ActorId::new("sender-actor");
        let receiver_id = ActorId::new("receiver-actor");
        let envelope = Arc::new(Envelope::new(TestEvent(42), sender_id.clone()));
        let entry = EventEntry::new(envelope, Arc::new(DefaultTopic), receiver_id.clone());
        (entry, sender_id, receiver_id)
    }

    #[test]
    fn id_returns_envelope_id() {
        let (entry, _, _) = make_entry();
        // ID should be non-zero (generated)
        assert_ne!(entry.id().value(), 0);
    }

    #[test]
    fn payload_returns_event() {
        let (entry, _, _) = make_entry();
        assert_eq!(entry.payload().0, 42);
    }

    #[test]
    fn meta_returns_envelope_meta() {
        let (entry, _, _) = make_entry();
        assert_eq!(entry.meta().actor_name(), "sender-actor");
    }

    #[test]
    fn topic_returns_routing_topic() {
        let (entry, _, _) = make_entry();
        assert_eq!(*entry.topic(), DefaultTopic);
    }

    #[test]
    fn sender_returns_sender_name() {
        let (entry, _, _) = make_entry();
        assert_eq!(entry.sender().as_str(), "sender-actor");
    }

    #[test]
    fn receiver_returns_receiver_name() {
        let (entry, _, _) = make_entry();
        assert_eq!(entry.receiver().as_str(), "receiver-actor");
    }

    #[test]
    fn receiver_actor_eq_matches_correctly() {
        let (entry, _, receiver_id) = make_entry();
        let not_matching = ActorId::new("other-actor");

        assert!(entry.receiver_actor_eq(&receiver_id));
        assert!(!entry.receiver_actor_eq(&not_matching));
    }

    #[test]
    fn sender_actor_eq_matches_correctly() {
        let (entry, sender_id, _) = make_entry();
        let not_matching = ActorId::new("other-actor");

        assert!(entry.sender_actor_eq(&sender_id));
        assert!(!entry.sender_actor_eq(&not_matching));
    }
}
