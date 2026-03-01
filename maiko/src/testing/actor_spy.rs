use std::fmt;

use crate::{
    ActorId, Event, Topic,
    monitors::ActorMonitor,
    testing::{EventEntry, EventQuery, EventRecords},
};

/// A spy for observing events from the perspective of a specific actor.
///
/// Provides methods to inspect:
/// - Events received by this actor (inbound)
/// - Events sent by this actor (outbound)
/// - Which actors this actor communicated with
pub struct ActorSpy<E: Event, T: Topic<E>> {
    actor: ActorId,
    inbound: EventQuery<E, T>,
    outbound: EventQuery<E, T>,
    actor_monitor: ActorMonitor,
}

impl<E: Event, T: Topic<E>> fmt::Debug for ActorSpy<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lifecycle = if self.is_running() {
            "running"
        } else if self.is_stopped() {
            "stopped"
        } else {
            "unknown"
        };
        f.debug_struct("ActorSpy")
            .field("actor", &self.actor)
            .field("lifecycle", &lifecycle)
            .field("inbound", &self.inbound)
            .field("outbound", &self.outbound)
            .finish()
    }
}

impl<E: Event, T: Topic<E>> ActorSpy<E, T> {
    pub(crate) fn new(
        records: EventRecords<E, T>,
        actor: ActorId,
        actor_monitor: ActorMonitor,
    ) -> Self {
        let inbound = EventQuery::new(records.clone()).received_by(&actor);
        let outbound = EventQuery::new(records).sent_by(&actor);

        Self {
            actor,
            inbound,
            outbound,
            actor_monitor,
        }
    }

    // ==================== Lifecycle ====================

    /// Returns `true` if this actor is currently running.
    ///
    /// An actor is running if it has been registered with the supervisor
    /// and has not yet stopped.
    pub fn is_running(&self) -> bool {
        self.actor_monitor.is_alive(&self.actor)
    }

    /// Returns `true` if this actor was registered and has since stopped.
    ///
    /// Returns `false` for actors that were never registered or are still
    /// running.
    pub fn is_stopped(&self) -> bool {
        self.actor_monitor.is_stopped(&self.actor)
    }

    // ==================== Inbound (events received by this actor) ====================

    /// Returns a query for events received by this actor.
    ///
    /// Use this to further filter or inspect inbound events.
    pub fn inbound(&self) -> EventQuery<E, T> {
        self.inbound.clone()
    }

    /// Returns the number of events received by this actor.
    pub fn events_received(&self) -> usize {
        self.inbound.count()
    }

    /// Returns the last event received by this actor.
    pub fn last_received(&self) -> Option<EventEntry<E, T>> {
        self.inbound.last()
    }

    /// Returns the names of actors that sent events to this actor.
    pub fn received_from(&self) -> Vec<ActorId> {
        distinct_by(&self.inbound, |e| e.meta().actor_id().clone())
    }

    /// Returns the count of distinct actors that sent events to this actor.
    pub fn sender_count(&self) -> usize {
        self.received_from().len()
    }

    // ==================== Outbound (events sent by this actor) ====================

    /// Returns a query for events sent by this actor.
    ///
    /// Use this to further filter or inspect outbound events.
    pub fn outbound(&self) -> EventQuery<E, T> {
        self.outbound.clone()
    }

    /// Returns the number of distinct events sent by this actor.
    ///
    /// Note: This counts unique event IDs, not deliveries. A single event
    /// delivered to multiple actors counts as one.
    pub fn events_sent(&self) -> usize {
        distinct_by(&self.outbound, |e| e.id()).len()
    }

    /// Returns the last event sent by this actor.
    pub fn last_sent(&self) -> Option<EventEntry<E, T>> {
        self.outbound.last()
    }

    /// Returns the names of actors that received events from this actor.
    pub fn sent_to(&self) -> Vec<ActorId> {
        distinct_by(&self.outbound, |e| e.actor_id.clone())
    }

    /// Returns the count of distinct actors that received events from this actor.
    pub fn receiver_count(&self) -> usize {
        self.sent_to().len()
    }
}

/// Helper to collect distinct values from a query using a mapper function.
fn distinct_by<E, T, R, F>(query: &EventQuery<E, T>, mapper: F) -> Vec<R>
where
    E: Event,
    T: Topic<E>,
    R: std::hash::Hash + std::cmp::Eq,
    F: Fn(&EventEntry<E, T>) -> R,
{
    use std::collections::HashSet;
    query
        .iter()
        .map(mapper)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ActorId, DefaultTopic, Envelope, Event, monitoring::Monitor, monitors::ActorMonitor,
    };

    #[derive(Clone, Debug)]
    struct TestEvent(i32);
    impl Event for TestEvent {}

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
    ) -> EventEntry<TestEvent, DefaultTopic> {
        let envelope = Arc::new(Envelope::new(event, sender.clone()));
        EventEntry::new(envelope, Arc::new(DefaultTopic), receiver.clone())
    }

    fn sample_records_with_actors(actors: &TestActors) -> EventRecords<TestEvent, DefaultTopic> {
        // Scenario: alice sends to bob and charlie, bob sends to alice
        Arc::new(vec![
            make_entry(TestEvent(1), &actors.alice, &actors.bob),
            make_entry(TestEvent(2), &actors.alice, &actors.charlie),
            make_entry(TestEvent(3), &actors.bob, &actors.alice),
            make_entry(TestEvent(4), &actors.charlie, &actors.alice),
        ])
    }

    // ==================== Inbound Tests ====================

    #[test]
    fn inbound_returns_events_received_by_actor() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        assert_eq!(spy.inbound().count(), 2); // received from bob and charlie
    }

    #[test]
    fn events_received_returns_received_event_count() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.bob,
            ActorMonitor::new(),
        );
        assert_eq!(spy.events_received(), 1); // received from alice
    }

    #[test]
    fn last_received_returns_most_recent_inbound() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        let last = spy.last_received().unwrap();
        assert_eq!(last.payload().0, 4); // from charlie
    }

    #[test]
    fn last_received_returns_none_when_no_inbound() {
        let actors = TestActors::new();
        let unknown = ActorId::new("unknown");
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            unknown,
            ActorMonitor::new(),
        );
        assert!(spy.last_received().is_none());
    }

    #[test]
    fn received_from_returns_unique_senders() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        let senders = spy.received_from();
        assert_eq!(senders.len(), 2);
        assert!(senders.iter().any(|s| s.as_str() == "bob"));
        assert!(senders.iter().any(|s| s.as_str() == "charlie"));
    }

    #[test]
    fn sender_count_returns_unique_sender_count() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        assert_eq!(spy.sender_count(), 2);
    }

    // ==================== Outbound Tests ====================

    #[test]
    fn outbound_returns_events_sent_by_actor() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        assert_eq!(spy.outbound().count(), 2); // sent to bob and charlie
    }

    #[test]
    fn events_sent_returns_unique_event_count() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        // 2 distinct events sent (different IDs)
        assert_eq!(spy.events_sent(), 2);
    }

    #[test]
    fn events_sent_deduplicates_same_event_to_multiple_receivers() {
        let alice = ActorId::new("alice");
        let bob = ActorId::new("bob");
        let charlie = ActorId::new("charlie");
        // Same event delivered to multiple actors
        let envelope = Arc::new(Envelope::new(TestEvent(1), alice.clone()));
        let topic = Arc::new(DefaultTopic);
        let records = Arc::new(vec![
            EventEntry::new(envelope.clone(), topic.clone(), bob),
            EventEntry::new(envelope, topic, charlie),
        ]);

        let spy = ActorSpy::new(records, alice, ActorMonitor::new());
        // Only 1 unique event sent, even though delivered to 2 actors
        assert_eq!(spy.events_sent(), 1);
    }

    #[test]
    fn last_sent_returns_most_recent_outbound() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        let last = spy.last_sent().unwrap();
        assert_eq!(last.payload().0, 2); // to charlie
    }

    #[test]
    fn last_sent_returns_none_when_no_outbound() {
        let actors = TestActors::new();
        let unknown = ActorId::new("unknown");
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            unknown,
            ActorMonitor::new(),
        );
        assert!(spy.last_sent().is_none());
    }

    #[test]
    fn sent_to_returns_unique_receivers() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        let receivers = spy.sent_to();
        assert_eq!(receivers.len(), 2);
        assert!(receivers.iter().any(|r| r.as_str() == "bob"));
        assert!(receivers.iter().any(|r| r.as_str() == "charlie"));
    }

    #[test]
    fn receiver_count_returns_unique_receiver_count() {
        let actors = TestActors::new();
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            actors.alice,
            ActorMonitor::new(),
        );
        assert_eq!(spy.receiver_count(), 2);
    }

    #[test]
    fn actor_with_no_activity_has_zero_counts() {
        let actors = TestActors::new();
        let unknown = ActorId::new("unknown");
        let spy = ActorSpy::new(
            sample_records_with_actors(&actors),
            unknown,
            ActorMonitor::new(),
        );
        assert_eq!(spy.events_received(), 0);
        assert_eq!(spy.events_sent(), 0);
        assert_eq!(spy.sender_count(), 0);
        assert_eq!(spy.receiver_count(), 0);
    }

    // ==================== Lifecycle Tests ====================

    #[test]
    fn is_running_returns_true_for_active_actor() {
        let monitor = ActorMonitor::new();
        let alice = ActorId::new("alice");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&alice);

        let spy = ActorSpy::new(
            Arc::new(Vec::<EventEntry<TestEvent, DefaultTopic>>::new()),
            alice,
            monitor,
        );
        assert!(spy.is_running());
        assert!(!spy.is_stopped());
    }

    #[test]
    fn is_stopped_returns_true_after_actor_stops() {
        let monitor = ActorMonitor::new();
        let alice = ActorId::new("alice");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&alice);
        m.on_actor_stop(&alice);

        let spy = ActorSpy::new(
            Arc::new(Vec::<EventEntry<TestEvent, DefaultTopic>>::new()),
            alice,
            monitor,
        );
        assert!(!spy.is_running());
        assert!(spy.is_stopped());
    }

    #[test]
    fn unknown_actor_is_neither_running_nor_stopped() {
        let monitor = ActorMonitor::new();
        let unknown = ActorId::new("unknown");

        let spy = ActorSpy::new(
            Arc::new(Vec::<EventEntry<TestEvent, DefaultTopic>>::new()),
            unknown,
            monitor,
        );
        assert!(!spy.is_running());
        assert!(!spy.is_stopped());
    }
}
