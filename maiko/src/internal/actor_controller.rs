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
        self.actor.on_start().await?;
        let mut step_handler = StepHandler::default();
        loop {
            select! {
                biased;

                cmd_res = self.command_rx.recv() => match cmd_res {
                    Ok(cmd) => match cmd {
                        Command::StopActor(ref id) if id == self.ctx.actor_id() => break,
                        Command::StopRuntime => break,
                        _ => {}
                    }
                    Err(e) => return Err(Error::internal(e))
                },

                Some(event) = self.receiver.recv() => {
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

                _ = async {
                    if let Some(backoff_sleep) = step_handler.backoff.as_mut() {
                        backoff_sleep.as_mut().await;
                    }
                }, if step_handler.is_delayed() => {
                    let _ = step_handler.backoff.take();
                    match self.actor.step().await {
                        Ok(action) => handle_step_action(action, &mut step_handler).await,
                        Err(e) => {
                            #[cfg(feature = "monitoring")]
                            self.notify_error(&e);

                            self.actor.on_error(e)?;
                            step_handler.reset();
                        }
                     }
                }

                res = self.actor.step(), if step_handler.can_step() => {
                     match res {
                        Ok(action) => handle_step_action(action, &mut step_handler).await,
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
    async fn test_monitoring_uses_event_topic_for_stage2_try_recv() -> anyhow::Result<()> {
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
