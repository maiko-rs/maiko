use std::future;
use std::sync::Arc;

use tokio::{
    select,
    sync::{broadcast, mpsc::Receiver},
};

use crate::{
    Actor, Context, Envelope, Error, Result, StepAction, Topic,
    internal::{Command, StepHandler, StepPause},
};

#[cfg(feature = "monitoring")]
use crate::monitoring::{MonitoringEvent, MonitoringSink};

pub(crate) struct ActorController<A: Actor, T: Topic<A::Event>> {
    pub(crate) actor: A,
    pub(crate) receiver: Receiver<Arc<Envelope<A::Event>>>,
    pub(crate) ctx: Context<A::Event>,
    pub(crate) max_events_per_tick: usize,
    pub(crate) command_rx: broadcast::Receiver<Command>,

    #[cfg(feature = "monitoring")]
    pub(crate) monitoring: MonitoringSink<A::Event, T>,

    pub(crate) _topic: std::marker::PhantomData<fn() -> T>,
}

impl<A: Actor, T: Topic<A::Event>> ActorController<A, T> {
    pub async fn run(&mut self) -> Result {
        // `select!` decides what should happen next without running actor work
        // directly inside branch futures. The selected action is executed below
        // in ordinary sequential code, which keeps borrow scopes simple.
        enum Next<E> {
            Continue,
            Stop,
            HandleEvent(Arc<Envelope<E>>),
            RunStep { clear_backoff: bool },
        }

        self.actor.on_start().await?;
        let mut step_handler = StepHandler::default();
        loop {
            let next = select! {
                biased;

                cmd_res = self.command_rx.recv() => match cmd_res {
                    Ok(cmd) => match cmd {
                        Command::StopActor(ref id) if id == self.ctx.actor_id() => Next::Stop,
                        Command::StopRuntime => Next::Stop,
                        _ => Next::Continue,
                    }
                    Err(e) => return Err(Error::internal(e)),
                },

                Some(event) = self.receiver.recv() => Next::HandleEvent(event),

                _ = async {
                    if let Some(backoff_sleep) = step_handler.backoff.as_mut() {
                        backoff_sleep.as_mut().await;
                    }
                }, if step_handler.is_delayed() => Next::RunStep { clear_backoff: true },

                _ = future::ready(()), if step_handler.can_step() => Next::RunStep { clear_backoff: false },
            };

            match next {
                Next::Continue => continue,
                Next::Stop => break,
                Next::HandleEvent(event) => {
                    self.handle_incoming_event(event).await?;

                    let mut cnt = 1;
                    while let Ok(event) = self.receiver.try_recv() {
                        self.handle_incoming_event(event).await?;
                        cnt += 1;
                        if cnt == self.max_events_per_tick {
                            break;
                        }
                    }
                    if step_handler.pause == StepPause::AwaitEvent {
                        step_handler.pause = StepPause::None;
                    }
                }
                Next::RunStep { clear_backoff } => {
                    if clear_backoff {
                        let _ = step_handler.backoff.take();
                    }

                    #[cfg(feature = "monitoring")]
                    self.notify_step_enter();

                    match self.actor.step().await {
                        Ok(action) => {
                            #[cfg(feature = "monitoring")]
                            self.notify_step_exit(action);

                            handle_step_action(action, &mut step_handler).await
                        }
                        Err(e) => {
                            #[cfg(feature = "monitoring")]
                            self.notify_error(&e);

                            self.actor.on_error(e)?;
                            step_handler.reset();
                        }
                    }
                }
            }
        }

        let res = self.actor.on_shutdown().await;

        #[cfg(feature = "monitoring")]
        self.notify_exit();

        res
    }

    #[inline]
    fn handle_error<R>(&mut self, result: Result<R>) -> Result {
        if let Err(e) = result {
            #[cfg(feature = "monitoring")]
            self.notify_error(&e);

            self.actor.on_error(e)?;
        }
        Ok(())
    }

    #[inline]
    async fn handle_incoming_event(&mut self, event: Arc<Envelope<A::Event>>) -> Result {
        #[cfg(feature = "monitoring")]
        let topic = {
            let topic = Arc::new(T::from_event(event.event()));
            self.notify_event_delivered(&event, &topic);
            topic
        };

        let res = self.actor.handle_event(&event).await;

        #[cfg(feature = "monitoring")]
        self.notify_event_handled(&event, &topic);

        self.handle_error(res)
    }
}

async fn handle_step_action(step_action: StepAction, step_handler: &mut StepHandler) {
    let pause = match step_action {
        crate::StepAction::Continue => StepPause::None,
        crate::StepAction::Yield => {
            tokio::task::yield_now().await;
            StepPause::None
        }
        crate::StepAction::AwaitEvent => StepPause::AwaitEvent,
        crate::StepAction::Backoff(duration) => {
            step_handler
                .backoff
                .replace(Box::pin(tokio::time::sleep(duration)));
            StepPause::None
        }
        crate::StepAction::Never => StepPause::Suppressed,
    };
    step_handler.pause = pause;
}

#[cfg(feature = "monitoring")]
impl<A: Actor, T: Topic<A::Event>> ActorController<A, T> {
    #[inline]
    fn notify_event_delivered(&self, event: &Arc<Envelope<A::Event>>, topic: &Arc<T>) {
        if self.monitoring.is_active() {
            self.monitoring.send(MonitoringEvent::EventDelivered(
                event.clone(),
                topic.clone(),
                self.ctx.actor_id().clone(),
            ));
        }
    }

    #[inline]
    fn notify_event_handled(&self, event: &Arc<Envelope<A::Event>>, topic: &Arc<T>) {
        if self.monitoring.is_active() {
            self.monitoring.send(MonitoringEvent::EventHandled(
                event.clone(),
                topic.clone(),
                self.ctx.actor_id().clone(),
            ));
        }
    }

    #[inline]
    fn notify_error(&self, error: &crate::Error) {
        if self.monitoring.is_active() {
            self.monitoring.send(MonitoringEvent::Error(
                error.to_string().into(),
                self.ctx.actor_id().clone(),
            ));
        }
    }

    #[inline]
    fn notify_exit(&self) {
        if self.monitoring.is_active() {
            self.monitoring
                .send(MonitoringEvent::ActorStopped(self.ctx.actor_id().clone()));
        }
    }

    #[inline]
    fn notify_step_enter(&self) {
        if self.monitoring.is_active() {
            self.monitoring
                .send(MonitoringEvent::StepEnter(self.ctx.actor_id().clone()));
        }
    }

    #[inline]
    fn notify_step_exit(&self, step_action: StepAction) {
        if self.monitoring.is_active() {
            self.monitoring.send(MonitoringEvent::StepExit(
                step_action,
                self.ctx.actor_id().clone(),
            ));
        }
    }
}

#[cfg(all(test, feature = "test-harness"))]
mod tests {
    use std::time::Duration;

    use crate::{Actor, ActorId, Envelope, Event, Supervisor, Topic, testing::Harness};

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum TestEvent {
        A,
        B,
    }
    impl Event for TestEvent {}

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum TestTopic {
        A,
        B,
    }
    impl Topic<TestEvent> for TestTopic {
        fn from_event(event: &TestEvent) -> Self {
            match event {
                TestEvent::A => TestTopic::A,
                TestEvent::B => TestTopic::B,
            }
        }
    }

    #[derive(Default)]
    struct SlowFirstEventActor {
        handled: usize,
    }

    impl Actor for SlowFirstEventActor {
        type Event = TestEvent;

        async fn handle_event(&mut self, _event: &Envelope<Self::Event>) -> crate::Result {
            // Delay first event so second one is already queued and consumed from
            // Stage 2 (`try_recv`) in the same tick.
            self.handled += 1;
            if self.handled == 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_monitoring_uses_event_topic_for_stage2_try_recv() -> crate::Result<()> {
        let mut sup = Supervisor::<TestEvent, TestTopic>::default();
        let receiver = sup
            .build_actor("receiver", |_| SlowFirstEventActor::default())
            .topics(&[TestTopic::A, TestTopic::B])
            .with_config(|c| c.with_max_events_per_tick(16)) // Make sure we process events back to back.
            .build()?;

        let mut test = Harness::new(&mut sup).await;
        test.record().await;

        // Queue both events before start so they are processed back-to-back.
        let sender = ActorId::new("sender");
        let _ = test.send_as(&sender, TestEvent::A).await?;
        let _ = test.send_as(&sender, TestEvent::B).await?;

        sup.start().await?;

        test.settle_on(|events| {
            events
                .received_by(&receiver)
                .matching_event(|e| matches!(e, TestEvent::B))
                .with_topic(TestTopic::B)
                .count()
                >= 1
        })
        .within(Duration::from_secs(1))
        .await?;

        assert_eq!(
            test.events()
                .received_by(&receiver)
                .matching_event(|e| matches!(e, TestEvent::A))
                .with_topic(TestTopic::A)
                .count(),
            1
        );
        assert_eq!(
            test.events()
                .received_by(&receiver)
                .matching_event(|e| matches!(e, TestEvent::B))
                .with_topic(TestTopic::B)
                .count(),
            1
        );

        sup.stop().await?;

        Ok(())
    }
}

#[cfg(all(test, feature = "monitoring"))]
mod monitoring_tests {
    //! Monitoring-specific regression tests for [`crate::internal::ActorController`].
    //!
    //! These cover runtime signals that are only emitted when the
    //! `monitoring` feature is enabled, such as step enter/exit callbacks.

    use crate::{
        Actor, ActorId, DefaultTopic, Envelope, Event, StepAction, Subscribe, Supervisor,
        monitoring::Monitor,
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::time;

    #[derive(Clone, Debug)]
    struct TestEvent;
    impl Event for TestEvent {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum StepRecord {
        Enter(ActorId),
        Exit(StepAction, ActorId),
    }

    struct StepMonitor {
        records: Arc<Mutex<Vec<StepRecord>>>,
    }

    impl Monitor<TestEvent, DefaultTopic> for StepMonitor {
        fn on_step_enter(&self, actor_id: &ActorId) {
            self.records
                .lock()
                .unwrap()
                .push(StepRecord::Enter(actor_id.clone()));
        }

        fn on_step_exit(&self, step_action: &StepAction, actor_id: &ActorId) {
            self.records
                .lock()
                .unwrap()
                .push(StepRecord::Exit(*step_action, actor_id.clone()));
        }
    }

    struct BackoffThenAwaitActor {
        steps: usize,
    }

    impl Actor for BackoffThenAwaitActor {
        type Event = TestEvent;

        async fn handle_event(&mut self, _event: &Envelope<Self::Event>) -> crate::Result {
            Ok(())
        }

        async fn step(&mut self) -> crate::Result<StepAction> {
            self.steps += 1;
            Ok(match self.steps {
                1 => StepAction::Backoff(Duration::from_millis(1)),
                _ => StepAction::AwaitEvent,
            })
        }
    }

    #[tokio::test]
    async fn test_monitoring_emits_step_enter_and_exit() -> crate::Result<()> {
        let mut sup = Supervisor::<TestEvent>::default();
        let records = Arc::new(Mutex::new(Vec::new()));
        let handle = sup
            .monitors()
            .add(StepMonitor {
                records: records.clone(),
            })
            .await;

        sup.add_actor(
            "stepper",
            |_| BackoffThenAwaitActor { steps: 0 },
            Subscribe::none(),
        )?;
        sup.start().await?;

        time::sleep(Duration::from_millis(10)).await;
        handle.flush(Duration::from_millis(1)).await;

        let stepper = ActorId::new("stepper");
        let expected = vec![
            StepRecord::Enter(stepper.clone()),
            StepRecord::Exit(
                StepAction::Backoff(Duration::from_millis(1)),
                stepper.clone(),
            ),
            StepRecord::Enter(stepper.clone()),
            StepRecord::Exit(StepAction::AwaitEvent, stepper),
        ];

        assert_eq!(*records.lock().unwrap(), expected);

        sup.stop().await
    }
}
