use std::fmt;

use crate::{
    ActorId, Event, Topic,
    testing::{EventQuery, EventRecords},
};

/// A spy for observing events on a specific topic.
///
/// Provides methods to inspect:
/// - Whether events were published to this topic
/// - Which actors received events on this topic
pub struct TopicSpy<E: Event, T: Topic<E>> {
    query: EventQuery<E, T>,
}

impl<E: Event, T: Topic<E>> fmt::Debug for TopicSpy<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TopicSpy").finish_non_exhaustive()
    }
}

impl<E: Event, T: Topic<E>> TopicSpy<E, T> {
    pub(crate) fn new(records: EventRecords<E, T>, topic: T) -> Self {
        Self {
            query: EventQuery::new(records).with_topic(topic),
        }
    }

    /// Returns true if any events were published to this topic.
    pub fn was_published(&self) -> bool {
        !self.query.is_empty()
    }

    /// Returns the number of event deliveries on this topic.
    ///
    /// Note: A single event delivered to multiple actors counts multiple times.
    pub fn event_count(&self) -> usize {
        self.query.count()
    }

    /// Returns the names of actors that received events on this topic.
    pub fn receivers(&self) -> Vec<ActorId> {
        use std::collections::HashSet;
        self.query
            .iter()
            .map(|e| e.actor_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Returns the number of distinct actors that received events on this topic.
    pub fn receiver_count(&self) -> usize {
        self.receivers().len()
    }

    /// Returns a query for further filtering events on this topic.
    pub fn events(&self) -> EventQuery<E, T> {
        self.query.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::EventEntry;
    use crate::{ActorId, Envelope, Event};

    #[derive(Clone, Debug)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

    #[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
    enum TestTopic {
        Data,
        Control,
    }

    impl Topic<TestEvent> for TestTopic {
        fn from_event(event: &TestEvent) -> Self {
            if event.0 < 100 {
                TestTopic::Data
            } else {
                TestTopic::Control
            }
        }
    }

    struct TestActors {
        alice: ActorId,
        bob: ActorId,
        charlie: ActorId,
    }

    impl TestActors {
        fn new() -> Self {
            Self {
                alice: ActorId::new("alice"),
                bob: ActorId::new("bob"),
                charlie: ActorId::new("charlie"),
            }
        }
    }

    fn make_entry(
        event: TestEvent,
        sender: &ActorId,
        receiver: &ActorId,
    ) -> EventEntry<TestEvent, TestTopic> {
        let topic = Arc::new(TestTopic::from_event(&event));
        let envelope = Arc::new(Envelope::new(event, sender.clone()));
        EventEntry::new(envelope, topic, receiver.clone())
    }

    fn sample_records_with_actors(actors: &TestActors) -> EventRecords<TestEvent, TestTopic> {
        Arc::new(vec![
            make_entry(TestEvent(1), &actors.alice, &actors.bob), // Data
            make_entry(TestEvent(2), &actors.alice, &actors.charlie), // Data
            make_entry(TestEvent(100), &actors.bob, &actors.alice), // Control
            make_entry(TestEvent(101), &actors.bob, &actors.charlie), // Control
        ])
    }

    #[test]
    fn was_published_returns_true_when_events_exist() {
        let actors = TestActors::new();
        let spy = TopicSpy::new(sample_records_with_actors(&actors), TestTopic::Data);
        assert!(spy.was_published());
    }

    #[test]
    fn was_published_returns_false_when_no_events() {
        let actors = TestActors::new();
        let records: EventRecords<TestEvent, TestTopic> = Arc::new(vec![
            make_entry(TestEvent(100), &actors.alice, &actors.bob), // Only Control
        ]);
        let spy = TopicSpy::new(records, TestTopic::Data);
        assert!(!spy.was_published());
    }

    #[test]
    fn event_count_returns_delivery_count() {
        let actors = TestActors::new();
        let spy = TopicSpy::new(sample_records_with_actors(&actors), TestTopic::Data);
        assert_eq!(spy.event_count(), 2);
    }

    #[test]
    fn event_count_counts_multiple_deliveries() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        // Same event delivered to multiple actors
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice));
        let topic = Arc::new(TestTopic::Data);
        let records = Arc::new(vec![
            EventEntry::new(envelope.clone(), topic.clone(), bob),
            EventEntry::new(envelope, topic, charlie),
        ]);

        let spy = TopicSpy::new(records, TestTopic::Data);
        assert_eq!(spy.event_count(), 2); // 2 deliveries
    }

    #[test]
    fn receivers_returns_unique_actors() {
        let actors = TestActors::new();
        let spy = TopicSpy::new(sample_records_with_actors(&actors), TestTopic::Control);
        let receivers = spy.receivers();
        assert_eq!(receivers.len(), 2);
        assert!(receivers.iter().any(|r| r.as_str() == "alice"));
        assert!(receivers.iter().any(|r| r.as_str() == "charlie"));
    }

    #[test]
    fn receiver_count_returns_unique_receiver_count() {
        let actors = TestActors::new();
        let spy = TopicSpy::new(sample_records_with_actors(&actors), TestTopic::Data);
        assert_eq!(spy.receiver_count(), 2); // bob and charlie
    }

    #[test]
    fn events_returns_filterable_query() {
        let actors = TestActors::new();
        let spy = TopicSpy::new(sample_records_with_actors(&actors), TestTopic::Data);
        let alice_events = spy.events().matching(|e| e.sender() == "alice").count();
        assert_eq!(alice_events, 2);
    }

    #[test]
    fn empty_topic_has_zero_counts() {
        let records: EventRecords<TestEvent, TestTopic> = Arc::new(vec![]);
        let spy = TopicSpy::new(records, TestTopic::Data);

        assert!(!spy.was_published());
        assert_eq!(spy.event_count(), 0);
        assert_eq!(spy.receiver_count(), 0);
        assert!(spy.receivers().is_empty());
    }
}
