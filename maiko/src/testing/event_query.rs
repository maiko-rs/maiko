use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use crate::{
    ActorId, Event, EventId, Label, Topic,
    testing::{EventEntry, EventMatcher, EventRecords},
};

type Filter<E, T> = Rc<dyn Fn(&EventEntry<E, T>) -> bool>;

/// A composable query builder for filtering and inspecting recorded events.
///
/// `EventQuery` provides a fluent API for filtering events by various criteria
/// (sender, receiver, topic, parent, timing) and terminal operations for
/// inspection (count, iteration, assertions).
///
/// # Example
///
/// ```ignore
/// let orders = test.events()
///     .sent_by(&trader)
///     .with_topic(MarketTopic::Order)
///     .after(&initial_tick)
///     .count();
/// ```
#[derive(Clone)]
pub struct EventQuery<E: Event, T: Topic<E>> {
    events: EventRecords<E, T>,
    filters: Vec<Filter<E, T>>,
}

impl<E: Event, T: Topic<E>> fmt::Debug for EventQuery<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventQuery")
            .field("records", &self.events.len())
            .field("filters", &self.filters.len())
            .finish()
    }
}

impl<E: Event, T: Topic<E>> EventQuery<E, T> {
    pub(crate) fn new(events: EventRecords<E, T>) -> Self {
        Self {
            events,
            filters: Vec::new(),
        }
    }

    fn add_filter<F>(&mut self, filter: F)
    where
        F: Fn(&EventEntry<E, T>) -> bool + 'static,
    {
        self.filters.push(Rc::new(filter));
    }

    fn apply_filters(&self) -> Vec<&EventEntry<E, T>> {
        self.events
            .iter()
            .filter(|e| self.filters.iter().all(|f| f(e)))
            .collect()
    }

    // ==================== Terminal Operations ====================

    /// Returns the number of events matching all filters.
    pub fn count(&self) -> usize {
        self.apply_filters().len()
    }

    /// Returns true if no events match the filters.
    pub fn is_empty(&self) -> bool {
        self.apply_filters().is_empty()
    }

    /// Returns true if any events match the filters.
    pub fn exists(&self) -> bool {
        !self.is_empty()
    }

    /// Returns the first matching event, if any.
    pub fn first(&self) -> Option<EventEntry<E, T>> {
        self.apply_filters().first().cloned().cloned()
    }

    /// Returns the last matching event, if any.
    pub fn last(&self) -> Option<EventEntry<E, T>> {
        self.apply_filters().last().cloned().cloned()
    }

    /// Returns the nth matching event (0-indexed), if any.
    pub fn nth(&self, index: usize) -> Option<EventEntry<E, T>> {
        self.apply_filters().get(index).cloned().cloned()
    }

    /// Collects unique events (deduplicated by event ID).
    ///
    /// When the same event is delivered to multiple actors, this returns
    /// only one entry per event ID, preserving the order of first occurrence.
    pub fn collect(&self) -> Vec<EventEntry<E, T>> {
        let mut seen = HashSet::new();
        self.apply_filters()
            .into_iter()
            .filter(|e| seen.insert(e.id()))
            .cloned()
            .collect()
    }

    /// Returns all matching delivery records, including duplicates.
    ///
    /// Unlike `collect()`, this includes every delivery of the same event
    /// to different actors.
    pub fn all_deliveries(&self) -> Vec<EventEntry<E, T>> {
        self.apply_filters().into_iter().cloned().collect()
    }

    /// Returns the unique senders (actor IDs) of matching events.
    pub fn senders(&self) -> Vec<ActorId> {
        let mut seen = HashSet::new();
        self.apply_filters()
            .into_iter()
            .filter_map(|e| {
                let id = e.meta().actor_id().clone();
                if seen.insert(id.clone()) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the unique receivers (actor IDs) of matching events.
    pub fn receivers(&self) -> Vec<ActorId> {
        let mut seen = HashSet::new();
        self.apply_filters()
            .into_iter()
            .filter_map(|e| {
                let id = e.actor_id.clone();
                if seen.insert(id.clone()) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the number of distinct senders of matching events.
    pub fn sender_count(&self) -> usize {
        self.senders().len()
    }

    /// Returns the number of distinct receivers of matching events.
    pub fn receiver_count(&self) -> usize {
        self.receivers().len()
    }

    /// Returns a count of matching events grouped by label.
    pub fn count_by_label(&self) -> HashMap<String, usize>
    where
        E: Label,
    {
        let mut counts = HashMap::new();
        for entry in self.apply_filters() {
            *counts
                .entry(entry.payload().label().to_string())
                .or_insert(0) += 1;
        }
        counts
    }

    /// Returns true if all matching events satisfy the predicate.
    pub fn all(&self, predicate: impl Fn(&EventEntry<E, T>) -> bool) -> bool {
        self.apply_filters().into_iter().all(predicate)
    }

    /// Returns true if any matching event satisfies the predicate.
    pub fn any(&self, predicate: impl Fn(&EventEntry<E, T>) -> bool) -> bool {
        self.apply_filters().into_iter().any(predicate)
    }

    /// Returns an iterator over references to matching events.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &EventEntry<E, T>> {
        self.apply_filters().into_iter()
    }

    // ==================== Boolean Convenience Methods ====================

    /// Returns true if any event matches the given matcher.
    ///
    /// Accepts anything that converts to `EventMatcher`: `&str` or `String`
    /// (by label, requires `E: Label`), `EventId`, or `EventMatcher` directly.
    pub fn has_event(&self, matcher: impl Into<EventMatcher<E, T>>) -> bool {
        let matcher = matcher.into();
        self.clone()
            .matching(move |entry| matcher.matches(entry))
            .exists()
    }

    /// Returns true if any matching event was sent by the given actor.
    pub fn has_sender(&self, actor_id: &ActorId) -> bool {
        self.clone().sent_by(actor_id).exists()
    }

    /// Returns true if any matching event was received by the given actor.
    pub fn has_receiver(&self, actor_id: &ActorId) -> bool {
        self.clone().received_by(actor_id).exists()
    }

    /// Returns true if any matching event satisfies the predicate.
    pub fn has(&self, predicate: impl Fn(&EventEntry<E, T>) -> bool + 'static) -> bool {
        self.clone().matching(predicate).exists()
    }

    // ==================== Filter Operations ====================

    /// Filter to events sent by the specified actor.
    pub fn sent_by(mut self, actor_id: &ActorId) -> Self {
        let actor = actor_id.clone();
        self.add_filter(move |e| e.sender_actor_eq(&actor));
        self
    }

    /// Filter to events received by the specified actor.
    pub fn received_by(mut self, actor_id: &ActorId) -> Self {
        let actor = actor_id.clone();
        self.add_filter(move |e| e.receiver_actor_eq(&actor));
        self
    }

    /// Filter to events with the specified topic.
    pub fn with_topic(mut self, topic: T) -> Self {
        self.add_filter(move |e| *e.topic == topic);
        self
    }

    /// Filter to a specific event by ID.
    pub fn with_id(mut self, id: impl Into<EventId>) -> Self {
        let id = id.into();
        self.add_filter(move |e| e.event.id() == id);
        self
    }

    /// Filter using a custom predicate on the event entry.
    pub fn matching<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&EventEntry<E, T>) -> bool + 'static,
    {
        self.add_filter(predicate);
        self
    }

    /// Filter using a custom predicate on the event payload.
    ///
    /// This is a convenience method for filtering by event content without
    /// needing to navigate through the EventEntry structure.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let orders = test.events()
    ///     .matching_event(|e| matches!(e, MarketEvent::Order(_)))
    ///     .count();
    /// ```
    pub fn matching_event<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&E) -> bool + 'static,
    {
        self.add_filter(move |entry| predicate(entry.payload()));
        self
    }

    /// Filter to events occurring after the given event (by timestamp).
    pub fn after(mut self, event: &EventEntry<E, T>) -> Self {
        let timestamp = event.meta().timestamp();
        self.add_filter(move |e| e.meta().timestamp() > timestamp);
        self
    }

    /// Filter to events occurring before the given event (by timestamp).
    pub fn before(mut self, event: &EventEntry<E, T>) -> Self {
        let timestamp = event.meta().timestamp();
        self.add_filter(move |e| e.meta().timestamp() < timestamp);
        self
    }

    /// Filter to child events of the given parent event ID.
    pub fn children_of(mut self, id: impl Into<EventId>) -> Self {
        let parent_id = id.into();
        self.add_filter(move |e| e.meta().parent_id() == Some(parent_id));
        self
    }

    /// Filter to events matching the given label.
    ///
    /// Requires the event type to implement `Label`.
    pub fn with_label(self, label: impl Into<std::borrow::Cow<'static, str>>) -> Self
    where
        E: Label,
    {
        let name: std::borrow::Cow<'static, str> = label.into();
        self.matching(move |entry| entry.payload().label() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefaultTopic, Envelope, Event};
    use std::borrow::Cow;
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    enum TestEvent {
        Ping,
        Pong,
        Data(i32),
    }
    impl Event for TestEvent {}

    impl Label for TestEvent {
        fn label(&self) -> Cow<'static, str> {
            Cow::Borrowed(match self {
                TestEvent::Ping => "Ping",
                TestEvent::Pong => "Pong",
                TestEvent::Data(_) => "Data",
            })
        }
    }

    /// Shared actor IDs for tests - ensures pointer equality works
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
        Arc::new(vec![
            make_entry(TestEvent::Ping, &actors.alice, &actors.bob),
            make_entry(TestEvent::Pong, &actors.bob, &actors.alice),
            make_entry(TestEvent::Data(42), &actors.alice, &actors.bob),
            make_entry(TestEvent::Data(42), &actors.alice, &actors.charlie),
            make_entry(TestEvent::Ping, &actors.charlie, &actors.alice),
        ])
    }

    #[test]
    fn count_returns_total_entries() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert_eq!(query.count(), 5);
    }

    #[test]
    fn is_empty_returns_false_when_records_exist() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(!query.is_empty());
    }

    #[test]
    fn is_empty_returns_true_for_empty_records() {
        let query: EventQuery<TestEvent, DefaultTopic> = EventQuery::new(Arc::new(vec![]));
        assert!(query.is_empty());
    }

    #[test]
    fn first_returns_first_entry() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let first = query.first().unwrap();
        assert_eq!(first.sender(), "alice");
        assert_eq!(first.receiver().as_str(), "bob");
        assert!(matches!(first.payload(), TestEvent::Ping));
    }

    #[test]
    fn last_returns_last_entry() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let last = query.last().unwrap();
        assert_eq!(last.sender(), "charlie");
        assert_eq!(last.receiver().as_str(), "alice");
    }

    #[test]
    fn nth_returns_correct_entry() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let second = query.nth(1).unwrap();
        assert_eq!(second.sender(), "bob");
        assert!(matches!(second.payload(), TestEvent::Pong));
    }

    #[test]
    fn nth_returns_none_for_out_of_bounds() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.nth(100).is_none());
    }

    #[test]
    fn all_deliveries_returns_all_matching() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let all = query.all_deliveries();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn sent_by_filters_by_sender() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).sent_by(&actors.alice);
        assert_eq!(query.count(), 3);
        assert!(query.all(|e| e.sender() == "alice"));
    }

    #[test]
    fn received_by_filters_by_receiver() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).received_by(&actors.bob);
        assert_eq!(query.count(), 2);
        assert!(query.all(|e| e.receiver().as_str() == "bob"));
    }

    #[test]
    fn with_topic_filters_by_topic() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).with_topic(DefaultTopic);
        // All entries have DefaultTopic
        assert_eq!(query.count(), 5);
    }

    #[test]
    fn with_id_filters_by_event_id() {
        let actors = TestActors::new();
        // Create same event delivered to multiple actors
        let envelope = Arc::new(Envelope::new(TestEvent::Data(42), actors.alice.clone()));
        let target_id = envelope.id();
        let topic = Arc::new(DefaultTopic);
        let records = Arc::new(vec![
            make_entry(TestEvent::Ping, &actors.bob, &actors.charlie),
            EventEntry::new(envelope.clone(), topic.clone(), actors.bob.clone()),
            EventEntry::new(envelope, topic, actors.charlie.clone()),
        ]);
        let query = EventQuery::new(records).with_id(target_id);
        // Same event delivered to bob and charlie
        assert_eq!(query.count(), 2);
    }

    #[test]
    fn matching_applies_custom_predicate() {
        let actors = TestActors::new();
        let query =
            EventQuery::new(sample_records_with_actors(&actors)).matching(|e| e.sender() == "bob");
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn matching_event_filters_by_payload() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors))
            .matching_event(|e| matches!(e, TestEvent::Data(_)));
        assert_eq!(query.count(), 2);
    }

    #[test]
    fn all_returns_true_when_all_match() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).sent_by(&actors.alice);
        assert!(query.all(|e| e.sender() == "alice"));
    }

    #[test]
    fn all_returns_false_when_any_doesnt_match() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(!query.all(|e| e.sender() == "alice"));
    }

    #[test]
    fn any_returns_true_when_some_match() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.any(|e| matches!(e.payload(), TestEvent::Pong)));
    }

    #[test]
    fn any_returns_false_when_none_match() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(!query.any(|e| matches!(e.payload(), TestEvent::Data(999))));
    }

    #[test]
    fn chained_filters_combine() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors))
            .sent_by(&actors.alice)
            .received_by(&actors.bob);
        assert_eq!(query.count(), 2);
    }

    #[test]
    fn after_filters_by_timestamp() {
        let actors = TestActors::new();
        let records = sample_records_with_actors(&actors);
        let pivot = records[1].clone();
        let query = EventQuery::new(records).after(&pivot);
        // Events after the second one (indices 2, 3, 4)
        assert_eq!(query.count(), 3);
    }

    #[test]
    fn before_filters_by_timestamp() {
        let actors = TestActors::new();
        let records = sample_records_with_actors(&actors);
        let pivot = records[3].clone();
        let query = EventQuery::new(records).before(&pivot);
        // Events before index 3 (indices 0, 1, 2)
        assert_eq!(query.count(), 3);
    }

    #[test]
    fn children_of_filters_by_parent_id() {
        let actors = TestActors::new();
        let topic = Arc::new(DefaultTopic);

        // Create a parent event and a child linked to it
        let parent_envelope = Arc::new(Envelope::new(TestEvent::Ping, actors.alice.clone()));
        let parent_id = parent_envelope.id();
        let parent = EventEntry::new(parent_envelope, topic.clone(), actors.bob.clone());

        let child_envelope =
            Arc::new(Envelope::new(TestEvent::Pong, actors.bob.clone()).with_parent_id(parent_id));
        let child = EventEntry::new(child_envelope, topic.clone(), actors.alice.clone());

        let unrelated_envelope =
            Arc::new(Envelope::new(TestEvent::Data(1), actors.charlie.clone()));
        let unrelated = EventEntry::new(unrelated_envelope, topic, actors.alice.clone());

        let records = Arc::new(vec![parent, child, unrelated]);
        let query = EventQuery::new(records).children_of(parent_id);
        assert_eq!(query.count(), 1);
        assert!(query.first().unwrap().sender() == "bob");
    }

    #[test]
    fn with_label_filters_by_label() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).with_label("Pong");
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn with_label_combined_with_other_filters() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors))
            .with_label("Ping")
            .sent_by(&actors.alice);
        assert_eq!(query.count(), 1);
    }

    #[test]
    fn collect_deduplicates_by_event_id() {
        let actors = TestActors::new();
        let topic = Arc::new(DefaultTopic);

        // Same event delivered to multiple actors
        let envelope = Arc::new(Envelope::new(TestEvent::Ping, actors.alice.clone()));
        let entry1 = EventEntry::new(envelope.clone(), topic.clone(), actors.bob.clone());
        let entry2 = EventEntry::new(envelope.clone(), topic.clone(), actors.charlie.clone());
        // Different event
        let envelope2 = Arc::new(Envelope::new(TestEvent::Pong, actors.alice.clone()));
        let entry3 = EventEntry::new(envelope2, topic, actors.bob.clone());

        let records = Arc::new(vec![entry1, entry2, entry3]);
        let query = EventQuery::new(records);

        // all_deliveries() returns all 3 entries
        assert_eq!(query.all_deliveries().len(), 3);

        // collect() returns 2 (one Ping, one Pong)
        assert_eq!(query.collect().len(), 2);
    }

    #[test]
    fn senders_returns_unique_sender_ids() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let senders = query.senders();
        assert_eq!(senders.len(), 3); // alice, bob, charlie
    }

    #[test]
    fn senders_with_filter_returns_filtered_senders() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).received_by(&actors.alice);
        let senders = query.senders();
        assert_eq!(senders.len(), 2); // bob and charlie send to alice
    }

    #[test]
    fn receivers_returns_unique_receiver_ids() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let receivers = query.receivers();
        assert_eq!(receivers.len(), 3); // alice, bob, charlie
    }

    #[test]
    fn count_by_label_groups_by_label() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        let counts = query.count_by_label();
        assert_eq!(counts["Ping"], 2);
        assert_eq!(counts["Pong"], 1);
        assert_eq!(counts["Data"], 2);
    }

    // ==================== sender_count() / receiver_count() ====================

    #[test]
    fn sender_count_returns_unique_sender_count() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert_eq!(query.sender_count(), 3); // alice, bob, charlie
    }

    #[test]
    fn receiver_count_returns_unique_receiver_count() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert_eq!(query.receiver_count(), 3); // alice, bob, charlie
    }

    #[test]
    fn sender_count_respects_filters() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).received_by(&actors.alice);
        assert_eq!(query.sender_count(), 2); // bob and charlie send to alice
    }

    #[test]
    fn receiver_count_respects_filters() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).sent_by(&actors.alice);
        assert_eq!(query.receiver_count(), 2); // alice sends to bob and charlie
    }

    // ==================== exists() ====================

    #[test]
    fn exists_returns_true_when_records_exist() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.exists());
    }

    #[test]
    fn exists_returns_false_for_empty_records() {
        let query: EventQuery<TestEvent, DefaultTopic> = EventQuery::new(Arc::new(vec![]));
        assert!(!query.exists());
    }

    #[test]
    fn exists_respects_filters() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).with_label("NonExistent");
        assert!(!query.exists());
    }

    // ==================== has_event() ====================

    #[test]
    fn has_event_by_label() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.has_event("Ping"));
        assert!(!query.has_event("NonExistent"));
    }

    #[test]
    fn has_event_by_matcher() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.has_event(EventMatcher::by_event(|e| {
            matches!(e, TestEvent::Data(42))
        })));
        assert!(!query.has_event(EventMatcher::by_event(|e| {
            matches!(e, TestEvent::Data(999))
        })));
    }

    #[test]
    fn has_event_respects_existing_filters() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors)).sent_by(&actors.bob);
        // Bob only sends Pong
        assert!(query.has_event("Pong"));
        assert!(!query.has_event("Ping"));
    }

    // ==================== has_sender() / has_receiver() / has() ====================

    #[test]
    fn has_sender_returns_true_for_matching_sender() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.has_sender(&actors.alice));
        assert!(query.has_sender(&actors.bob));

        let unknown = ActorId::new("unknown");
        assert!(!query.has_sender(&unknown));
    }

    #[test]
    fn has_receiver_returns_true_for_matching_receiver() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.has_receiver(&actors.bob));
        assert!(query.has_receiver(&actors.alice));

        let unknown = ActorId::new("unknown");
        assert!(!query.has_receiver(&unknown));
    }

    #[test]
    fn has_returns_true_for_matching_entry() {
        let actors = TestActors::new();
        let query = EventQuery::new(sample_records_with_actors(&actors));
        assert!(query.has(|e| e.sender() == "charlie"));
        assert!(!query.has(|e| e.sender() == "unknown"));
    }
}
