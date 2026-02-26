use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::mpsc::{Sender, UnboundedReceiver, unbounded_channel};

use crate::{
    ActorId, Envelope, EnvelopeBuilder, Event, EventId, Supervisor, Topic,
    monitoring::MonitorHandle,
    testing::{
        ActorSpy, EventChain, EventCollector, EventEntry, EventMatcher, EventQuery, EventRecords,
        EventSpy, TopicSpy, expectation::Expectation,
    },
};

/// Test harness for observing and asserting on event flow in a Maiko system.
///
/// The harness provides:
/// - Event injection via [`send_as`](Self::send_as)
/// - Recording control via [`record`](Self::record) / [`settle`](Self::settle)
/// - Condition-based settling via [`settle_on`](Self::settle_on)
/// - Query access via [`events`](Self::events), [`event`](Self::event), [`actor`](Self::actor), [`topic`](Self::topic)
///
/// # Example
///
/// ```ignore
/// // Create harness BEFORE starting supervisor
/// let mut test = Harness::new(&mut supervisor).await;
/// supervisor.start().await?;
///
/// test.record().await;
/// let id = test.send_as(&producer, MyEvent::Data(42)).await?;
/// test.settle().await;
///
/// assert!(test.event(id).was_delivered_to(&consumer));
/// assert_eq!(1, test.actor(&consumer).events_received());
/// ```
///
/// # Warning
///
/// **Do not use in production.** The test harness uses an unbounded channel
/// for event collection, which can lead to memory exhaustion under high load.
/// For production monitoring, use the [`monitoring`](crate::monitoring) API directly.
pub struct Harness<E: Event, T: Topic<E>> {
    pub(super) snapshot: Vec<EventEntry<E, T>>,
    records: EventRecords<E, T>,
    monitor_handle: MonitorHandle<E, T>,
    pub(super) receiver: UnboundedReceiver<EventEntry<E, T>>,
    actor_sender: Sender<Arc<Envelope<E>>>,
}

impl<E: Event, T: Topic<E>> fmt::Debug for Harness<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Harness")
            .field("snapshot", &self.snapshot.len())
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl<E: Event, T: Topic<E>> Harness<E, T> {
    /// Create a new test harness attached to the given supervisor.
    ///
    /// Must be called before `supervisor.start()`. The harness installs
    /// an internal monitor to capture event flow.
    pub async fn new(supervisor: &mut Supervisor<E, T>) -> Self {
        let (tx, rx) = unbounded_channel();
        let monitor = EventCollector::new(tx);
        let monitor_handle = supervisor.monitors().add(monitor).await;
        Self {
            snapshot: Vec::new(),
            records: Arc::new(Vec::new()),
            monitor_handle,
            receiver: rx,
            actor_sender: supervisor.sender.clone(),
        }
    }

    // ==================== Recording Control ====================

    /// Start recording events. Call before sending test events.
    ///
    /// Resumes the underlying monitor so that events flowing through the
    /// system are captured. Pair with [`settle`](Self::settle) or
    /// [`settle_on`](Self::settle_on) to end the recording phase.
    pub async fn record(&self) {
        self.monitor_handle.resume().await;
    }

    /// Drain events until silence, then pause the monitor and freeze the snapshot.
    ///
    /// Collects events until no new events arrive for 1ms (settle window),
    /// or until 10ms total have elapsed (max settle time). Then pauses the
    /// monitor and freezes the snapshot for querying.
    ///
    /// For chatty actors that continuously produce events, the max settle
    /// time prevents infinite waiting.
    ///
    /// After calling this, spy methods will query the captured snapshot.
    pub async fn settle(&mut self) {
        self.drain_until_quiet(Self::DEFAULT_SETTLE_WINDOW, Self::DEFAULT_MAX_SETTLE)
            .await;
        self.freeze().await;
    }

    /// Return a condition-based settle builder.
    ///
    /// The harness drains events and checks the provided predicate after
    /// each arrival. When the condition returns `true`, recording stops and
    /// the snapshot freezes. If the default 1-second timeout expires first,
    /// returns [`Error::SettleTimeout`](crate::Error::SettleTimeout).
    ///
    /// The condition receives an [`EventQuery`] built from all events
    /// recorded so far.
    ///
    /// # Example
    ///
    /// ```ignore
    /// test.settle_on(|events| events.with_label("HidReport").count() >= 5).await?;
    ///
    /// // With a custom timeout
    /// test.settle_on(|events| events.with_label("HidReport").count() >= 5)
    ///     .within(Duration::from_secs(3))
    ///     .await?;
    /// ```
    pub fn settle_on<F>(&mut self, condition: F) -> Expectation<'_, E, T, F>
    where
        F: Fn(EventQuery<E, T>) -> bool,
    {
        Expectation::new(self, condition)
    }

    /// Wait for a specific event to appear.
    ///
    /// Accepts anything that converts to `EventMatcher`: `&str` or `String`
    /// (by label, requires `E: Label`), `EventId`, or `EventMatcher` directly.
    /// Returns an `Expectation` so `.within()` and `.await` chain as usual.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // By label
    /// test.settle_on_event("Pong").await?;
    ///
    /// // By matcher
    /// test.settle_on_event(EventMatcher::by_event(|e| matches!(e, MyEvent::Pong)))
    ///     .await?;
    ///
    /// // With timeout
    /// test.settle_on_event("Pong").within(Duration::from_secs(3)).await?;
    /// ```
    pub fn settle_on_event<M>(
        &mut self,
        matcher: M,
    ) -> Expectation<'_, E, T, impl Fn(EventQuery<E, T>) -> bool>
    where
        M: Into<EventMatcher<E, T>>,
    {
        let matcher = matcher.into();
        self.settle_on(move |events| events.any(|entry| matcher.matches(entry)))
    }

    /// Clears all recorded events, resetting the harness for the next test phase.
    pub fn reset(&mut self) {
        self.snapshot.clear();
        self.records = Arc::new(Vec::new());
        while let Ok(_entry) = self.receiver.try_recv() {}
    }

    /// Default settle window: wait 1ms for quiet before considering settled.
    pub const DEFAULT_SETTLE_WINDOW: Duration = Duration::from_millis(1);

    /// Default max settle time: give up waiting after 10ms.
    pub const DEFAULT_MAX_SETTLE: Duration = Duration::from_millis(10);

    /// Drain events until the system is quiet (no new events within `settle_window`)
    /// or until `max_settle` total time has elapsed.
    pub(super) async fn drain_until_quiet(
        &mut self,
        settle_window: Duration,
        max_settle: Duration,
    ) {
        let deadline = Instant::now() + max_settle;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            let timeout = settle_window.min(remaining);
            match tokio::time::timeout(timeout, self.receiver.recv()).await {
                Ok(Some(entry)) => self.snapshot.push(entry),
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Quiet for settle_window - system settled
            }
        }
    }

    /// Pause the monitor and freeze the current snapshot for querying.
    pub(super) async fn freeze(&mut self) {
        self.monitor_handle.pause().await;
        self.records = Arc::new(std::mem::take(&mut self.snapshot));
    }

    // ==================== Event Injection ====================

    /// Send an event as if it came from the specified actor.
    ///
    /// Returns the event ID which can be used with [`event`](Self::event) to
    /// inspect delivery.
    pub async fn send_as<B: Into<EnvelopeBuilder<E>>>(
        &self,
        actor_id: &ActorId,
        builder: B,
    ) -> crate::Result<EventId> {
        let envelope = builder.into().with_actor_id(actor_id.clone()).build()?;
        let id = envelope.id();
        self.actor_sender.send(Arc::new(envelope)).await?;
        Ok(id)
    }

    // ==================== Query Access ====================

    /// Returns a query over all recorded events.
    ///
    /// This is the most flexible way to query events, allowing arbitrary
    /// filtering and inspection.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let orders = test.events()
    ///     .sent_by(&trader)
    ///     .matching_event(|e| matches!(e, MarketEvent::Order(_)))
    ///     .count();
    /// ```
    pub fn events(&self) -> EventQuery<E, T> {
        EventQuery::new(self.records.clone())
    }

    /// Returns a spy for observing a specific event by ID.
    ///
    /// Use this to inspect delivery and child events.
    pub fn event(&self, id: EventId) -> EventSpy<E, T> {
        EventSpy::new(self.records.clone(), id)
    }

    /// Returns a spy for observing events from a specific actor's perspective.
    ///
    /// Use this to inspect what an actor sent and received.
    pub fn actor(&self, actor: &ActorId) -> ActorSpy<E, T> {
        ActorSpy::new(self.records.clone(), actor.clone())
    }

    /// Returns a spy for observing events on a specific topic.
    ///
    /// Use this to inspect event flow through a topic.
    pub fn topic(&self, topic: T) -> TopicSpy<E, T> {
        TopicSpy::new(self.records.clone(), topic)
    }

    /// Returns an event chain for tracing event propagation from a root event.
    ///
    /// The chain captures all descendant events of the root (children, grandchildren, etc.)
    /// and provides methods to verify actor flow and event sequences.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let chain = test.chain(root_event_id);
    ///
    /// // Verify exact path from root to leaf
    /// assert!(chain.actors().exact(&[&scanner, &pipeline, &writer, &telemetry]));
    ///
    /// // Verify contiguous sub-path
    /// assert!(chain.actors().segment(&[&pipeline, &writer]));
    ///
    /// // Verify reachability (gaps allowed)
    /// assert!(chain.actors().passes_through(&[&scanner, &telemetry]));
    ///
    /// // Verify event sequence
    /// assert!(chain.events().segment(&["KeyPress", "HidReport"]));
    /// ```
    pub fn chain(&self, id: EventId) -> EventChain<E, T> {
        EventChain::new(self.records.clone(), id)
    }

    // ==================== Debugging ====================

    /// Print all recorded events to stdout for debugging.
    ///
    /// Shows sender, receiver, and event ID for each recorded delivery.
    pub fn dump(&self) {
        if self.records.is_empty() {
            println!("(no events recorded)");
            return;
        }
        println!("Recorded events ({} deliveries):", self.records.len());
        for (i, entry) in self.records.iter().enumerate() {
            println!(
                "  {}: [{}] --> [{}]  (id: {})",
                i,
                entry.sender(),
                entry.receiver(),
                entry.id(),
            );
        }
    }

    /// Returns the number of recorded events.
    pub fn event_count(&self) -> usize {
        self.records.len()
    }
}
