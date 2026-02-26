use std::{
    fmt,
    future::IntoFuture,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{Error, Event, Result, Topic, testing::EventQuery};

use super::Harness;

/// Default timeout for `settle_on` conditions.
pub const DEFAULT_EXPECT_TIMEOUT: Duration = Duration::from_secs(1);

/// A condition-based settle builder.
///
/// Created by [`Harness::settle_on`]. The harness drains events and checks
/// the user-provided predicate after each arrival. When the condition is met,
/// recording stops and the snapshot freezes. If the timeout expires first,
/// returns [`Error::SettleTimeout`].
///
/// # Example
///
/// ```ignore
/// // Wait until 5 HidReport events are recorded
/// test.settle_on(|events| events.with_label("HidReport").count() >= 5).await?;
///
/// // With a custom timeout
/// test.settle_on(|events| events.with_label("HidReport").count() >= 5)
///     .within(Duration::from_secs(3))
///     .await?;
/// ```
pub struct Expectation<'a, E: Event, T: Topic<E>, F> {
    harness: &'a mut Harness<E, T>,
    condition: F,
    timeout: Duration,
}

impl<'a, E: Event, T: Topic<E>, F> Expectation<'a, E, T, F>
where
    F: Fn(EventQuery<E, T>) -> bool,
{
    pub(crate) fn new(harness: &'a mut Harness<E, T>, condition: F) -> Self {
        Self {
            harness,
            condition,
            timeout: DEFAULT_EXPECT_TIMEOUT,
        }
    }

    /// Override the default 1-second timeout.
    pub fn within(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn run(self) -> Result {
        let deadline = Instant::now() + self.timeout;

        // Let in-flight events propagate through the async pipeline
        // (broker → actor → monitor → dispatcher → collector) before
        // entering the condition-check loop.
        self.harness
            .drain_until_quiet(
                Harness::<E, T>::DEFAULT_SETTLE_WINDOW,
                Harness::<E, T>::DEFAULT_MAX_SETTLE,
            )
            .await;

        if self.check_condition() {
            self.harness.freeze().await;
            return Ok(());
        }

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::SettleTimeout(
                    self.timeout,
                    self.harness.snapshot.len(),
                ));
            }

            match tokio::time::timeout(remaining, self.harness.receiver.recv()).await {
                Ok(Some(entry)) => {
                    self.harness.snapshot.push(entry);
                    // Drain any additional pending events before checking
                    while let Ok(entry) = self.harness.receiver.try_recv() {
                        self.harness.snapshot.push(entry);
                    }
                    if self.check_condition() {
                        self.harness.freeze().await;
                        return Ok(());
                    }
                }
                Ok(None) => {
                    // Channel closed - check one last time
                    if self.check_condition() {
                        self.harness.freeze().await;
                        return Ok(());
                    }
                    return Err(Error::SettleTimeout(
                        self.timeout,
                        self.harness.snapshot.len(),
                    ));
                }
                Err(_) => {
                    // Timeout expired
                    return Err(Error::SettleTimeout(
                        self.timeout,
                        self.harness.snapshot.len(),
                    ));
                }
            }
        }
    }

    fn check_condition(&self) -> bool {
        let query = EventQuery::new(Arc::new(self.harness.snapshot.clone()));
        (self.condition)(query)
    }
}

impl<'a, E: Event, T: Topic<E>, F> IntoFuture for Expectation<'a, E, T, F>
where
    F: Fn(EventQuery<E, T>) -> bool + 'a,
{
    type Output = Result;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.run())
    }
}

impl<E: Event, T: Topic<E>, F> fmt::Debug for Expectation<'_, E, T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expectation")
            .field("harness", &self.harness)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        Actor, Envelope, Event, Label, Result, Subscribe, Supervisor,
        testing::{EventMatcher, Harness},
    };
    use std::borrow::Cow;

    #[derive(Clone, Debug)]
    enum TestEvent {
        Ping,
        Pong,
    }
    impl Event for TestEvent {}
    impl Label for TestEvent {
        fn label(&self) -> Cow<'static, str> {
            match self {
                TestEvent::Ping => Cow::Borrowed("Ping"),
                TestEvent::Pong => Cow::Borrowed("Pong"),
            }
        }
    }

    /// Echo actor: receives Ping, sends Pong back.
    struct Echo {
        ctx: crate::Context<TestEvent>,
    }

    impl Actor for Echo {
        type Event = TestEvent;

        async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result {
            if matches!(envelope.event(), TestEvent::Ping) {
                self.ctx.send(TestEvent::Pong).await?;
            }
            Ok(())
        }
    }

    /// Sink actor: receives events but does nothing.
    struct Sink;
    impl Actor for Sink {
        type Event = TestEvent;
    }

    async fn setup() -> (Supervisor<TestEvent>, crate::ActorId, crate::ActorId) {
        let mut sup = Supervisor::<TestEvent>::default();
        let echo = sup
            .add_actor("echo", |ctx| Echo { ctx }, Subscribe::all())
            .unwrap();
        let sink = sup.add_actor("sink", |_| Sink, Subscribe::all()).unwrap();
        (sup, echo, sink)
    }

    #[tokio::test]
    async fn condition_met_immediately() {
        let (mut sup, _echo, sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        // Send an event, then settle_on with a trivially satisfiable condition.
        // The built-in drain lets the event propagate before checking.
        test.record().await;
        test.send_as(&sink, TestEvent::Ping).await.unwrap();
        test.settle_on(|events| events.exists()).await.unwrap();

        assert!(test.events().exists());
        sup.stop().await.unwrap();
    }

    #[tokio::test]
    async fn condition_met_after_several_events() {
        let (mut sup, _echo, sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;

        // Send 3 pings from sink. The broker skips self-delivery, so each
        // Ping goes only to echo. Echo handles it (sends Pong), and the Pong
        // goes only to sink. Total: 3 Ping deliveries + 3 Pong deliveries = 6.
        for _ in 0..3 {
            test.send_as(&sink, TestEvent::Ping).await.unwrap();
        }

        // Wait until we see all 6 deliveries (3 Ping→echo + 3 Pong→sink)
        test.settle_on(|events| events.with_label("Pong").count() >= 3)
            .await
            .unwrap();

        assert!(test.events().has_event("Ping"));
        assert!(test.events().has_event("Pong"));
        assert_eq!(test.events().with_label("Ping").count(), 3);
        assert_eq!(test.events().with_label("Pong").count(), 3);
        sup.stop().await.unwrap();
    }

    #[tokio::test]
    async fn timeout_expires() {
        let (mut sup, _echo, _sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;
        // No events sent - condition will never be met
        let result = test
            .settle_on(|events| events.count() >= 100)
            .within(Duration::from_millis(50))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, crate::Error::SettleTimeout(_, _)),
            "Expected SettleTimeout, got: {err:?}"
        );

        sup.stop().await.unwrap();
    }

    #[tokio::test]
    async fn channel_closed_returns_error() {
        let (mut sup, _echo, _sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;

        // Stop the supervisor (closes channels)
        sup.stop().await.unwrap();

        // settle_on should detect closed channel and fail
        let result = test
            .settle_on(|events| events.count() >= 100)
            .within(Duration::from_millis(100))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn within_overrides_default_timeout() {
        let (mut sup, _echo, _sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;

        let start = std::time::Instant::now();
        let result = test
            .settle_on(|events| events.count() >= 100)
            .within(Duration::from_millis(50))
            .await;

        let elapsed = start.elapsed();
        assert!(result.is_err());
        // Should have timed out around 50ms, not the default 1s
        assert!(
            elapsed < Duration::from_millis(200),
            "Should have timed out in ~50ms but took {:?}",
            elapsed
        );

        sup.stop().await.unwrap();
    }

    // ==================== settle_on_event ====================

    #[tokio::test]
    async fn settle_on_event_by_label() {
        let (mut sup, _echo, sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;
        // sink sends Ping → echo receives it → echo sends Pong
        test.send_as(&sink, TestEvent::Ping).await.unwrap();
        test.settle_on_event("Pong").await.unwrap();

        assert!(test.events().has_event("Pong"));
        sup.stop().await.unwrap();
    }

    #[tokio::test]
    async fn settle_on_event_with_timeout() {
        let (mut sup, _echo, _sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;
        // No events sent - condition will never be met
        let result = test
            .settle_on_event("Pong")
            .within(Duration::from_millis(50))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::Error::SettleTimeout(_, _)
        ));
        sup.stop().await.unwrap();
    }

    #[tokio::test]
    async fn settle_on_event_by_matcher() {
        let (mut sup, _echo, sink) = setup().await;
        let mut test = Harness::new(&mut sup).await;
        sup.start().await.unwrap();

        test.record().await;
        test.send_as(&sink, TestEvent::Ping).await.unwrap();
        test.settle_on_event(EventMatcher::by_event(|e| matches!(e, TestEvent::Pong)))
            .await
            .unwrap();

        assert!(test.events().with_label("Pong").exists());
        sup.stop().await.unwrap();
    }
}
