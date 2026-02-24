use std::fmt;

use crate::{
    ActorId, Event, EventId, Topic,
    testing::{EventQuery, EventRecords},
};

/// A spy for observing the delivery and effects of a specific event.
///
/// Provides methods to inspect:
/// - Whether and where the event was delivered
/// - Child events linked to this event via parent ID
pub struct EventSpy<E: Event, T: Topic<E>> {
    id: EventId,
    records: EventRecords<E, T>,
    query: EventQuery<E, T>,
}

impl<E: Event, T: Topic<E>> fmt::Debug for EventSpy<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventSpy")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<E: Event, T: Topic<E>> EventSpy<E, T> {
    pub(crate) fn new(records: EventRecords<E, T>, id: impl Into<EventId>) -> Self {
        let id = id.into();
        let query = EventQuery::new(records.clone()).with_id(id);
        Self { id, records, query }
    }

    /// Returns true if the event was delivered to at least one actor.
    pub fn was_delivered(&self) -> bool {
        !self.query.is_empty()
    }

    /// Returns true if the event was delivered to the specified actor.
    pub fn was_delivered_to(&self, actor_id: &ActorId) -> bool {
        self.query.clone().received_by(actor_id).count() > 0
    }

    /// Returns the name of the actor that sent this event.
    pub fn sender(&self) -> Option<ActorId> {
        self.query.first().map(|e| e.meta().actor_id().clone())
    }

    /// Returns the number of actors that received this event.
    pub fn receiver_count(&self) -> usize {
        self.receivers().len()
    }

    /// Returns the names of actors that received this event.
    pub fn receivers(&self) -> Vec<ActorId> {
        use std::collections::HashSet;
        self.query
            .iter()
            .map(|e| e.actor_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Returns a query for child events (events whose parent ID matches this event's ID).
    pub fn children(&self) -> EventQuery<E, T> {
        EventQuery::new(self.records.clone()).children_of(self.id)
    }

    /// Returns true if the event was NOT delivered to the specified actor.
    pub fn not_delivered_to(&self, actor_id: &ActorId) -> bool {
        !self.was_delivered_to(actor_id)
    }

    /// Returns true if the event was delivered to all specified actors.
    pub fn was_delivered_to_all(&self, actors: &[&ActorId]) -> bool {
        actors.iter().all(|a| self.was_delivered_to(a))
    }

    /// Returns the ratio of specified actors that received this event.
    ///
    /// Returns 1.0 if the actors slice is empty.
    pub fn delivery_ratio(&self, actors: &[&ActorId]) -> f64 {
        if actors.is_empty() {
            return 1.0;
        }
        let delivered = actors.iter().filter(|a| self.was_delivered_to(a)).count();
        delivered as f64 / actors.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::EventEntry;
    use crate::{ActorId, DefaultTopic, Envelope, Event};

    #[derive(Clone, Debug)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

    fn topic() -> Arc<DefaultTopic> {
        Arc::new(DefaultTopic)
    }

    #[test]
    fn was_delivered_returns_true_when_event_exists() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert!(spy.was_delivered());
    }

    #[test]
    fn was_delivered_returns_false_when_event_not_found() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, 99999u128);
        assert!(!spy.was_delivered());
    }

    #[test]
    fn was_delivered_to_returns_true_for_matching_receiver() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob.clone());
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert!(spy.was_delivered_to(&bob));
    }

    #[test]
    fn was_delivered_to_returns_false_for_non_matching_receiver() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert!(!spy.was_delivered_to(&charlie));
    }

    #[test]
    fn sender_returns_sender_name() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert_eq!(spy.sender().expect("sender should exist").as_str(), "alice");
    }

    #[test]
    fn receivers_returns_all_unique_receivers() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let t = topic();
        // Same event delivered to multiple actors
        let entry1 = EventEntry::new(envelope.clone(), t.clone(), bob.clone());
        let entry2 = EventEntry::new(envelope.clone(), t.clone(), charlie);
        let entry3 = EventEntry::new(envelope, t, bob); // duplicate
        let records = Arc::new(vec![entry1, entry2, entry3]);

        let spy = EventSpy::new(records, id);
        let receivers = spy.receivers();
        assert_eq!(receivers.len(), 2);
        assert!(receivers.iter().any(|r| r.as_str() == "bob"));
        assert!(receivers.iter().any(|r| r.as_str() == "charlie"));
    }

    #[test]
    fn receiver_count_returns_unique_receiver_count() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let t = topic();
        let entry1 = EventEntry::new(envelope.clone(), t.clone(), bob);
        let entry2 = EventEntry::new(envelope, t, charlie);
        let records = Arc::new(vec![entry1, entry2]);

        let spy = EventSpy::new(records, id);
        assert_eq!(spy.receiver_count(), 2);
    }

    #[test]
    fn children_returns_child_events() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let t = topic();

        // Parent event
        let parent = Arc::new(Envelope::new(TestEvent(1), alice.clone()));
        let parent_id = parent.id();
        let parent_entry = EventEntry::new(parent, t.clone(), bob.clone());

        // Child event linked to parent
        let child = Arc::new(Envelope::new(TestEvent(2), bob).with_parent_id(parent_id));
        let child_entry = EventEntry::new(child, t.clone(), alice.clone());

        // Unrelated event
        let unrelated = Arc::new(Envelope::new(TestEvent(3), charlie));
        let unrelated_entry = EventEntry::new(unrelated, t, alice);

        let records = Arc::new(vec![parent_entry, child_entry, unrelated_entry]);

        let spy = EventSpy::new(records, parent_id);
        let children = spy.children();
        assert_eq!(children.count(), 1);
        assert_eq!(children.first().unwrap().payload().0, 2);
    }

    #[test]
    fn not_delivered_to_returns_true_for_non_receiver() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert!(spy.not_delivered_to(&charlie));
    }

    #[test]
    fn was_delivered_to_all_returns_true_when_all_received() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let t = topic();
        let entry1 = EventEntry::new(envelope.clone(), t.clone(), bob.clone());
        let entry2 = EventEntry::new(envelope, t, charlie.clone());
        let records = Arc::new(vec![entry1, entry2]);

        let spy = EventSpy::new(records, id);
        assert!(spy.was_delivered_to_all(&[&bob, &charlie]));
    }

    #[test]
    fn was_delivered_to_all_returns_false_when_some_missing() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob.clone());
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert!(!spy.was_delivered_to_all(&[&bob, &charlie]));
    }

    #[test]
    fn delivery_ratio_returns_correct_ratio() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        let dave = ActorId::new("dave");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let t = topic();
        let entry1 = EventEntry::new(envelope.clone(), t.clone(), bob.clone());
        let entry2 = EventEntry::new(envelope, t, charlie.clone());
        let records = Arc::new(vec![entry1, entry2]);

        let spy = EventSpy::new(records, id);
        // 2 out of 3 actors received it
        let ratio = spy.delivery_ratio(&[&bob, &charlie, &dave]);
        assert!((ratio - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn delivery_ratio_returns_one_for_empty_slice() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let id = envelope.id();
        let entry = EventEntry::new(envelope, topic(), bob);
        let records = Arc::new(vec![entry]);

        let spy = EventSpy::new(records, id);
        assert_eq!(spy.delivery_ratio(&[]), 1.0);
    }
}
