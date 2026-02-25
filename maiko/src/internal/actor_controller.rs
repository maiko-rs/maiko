use std::sync::Arc;

use tokio::{
    select,
    sync::{broadcast, mpsc::Receiver},
};

use crate::{
    Actor, Context, Envelope, Result, StepAction, Topic,
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
    pub async fn run(&mut self) -> Result<()> {
        self.actor.on_start().await?;
        let mut step_handler = StepHandler::default();
        loop {
            select! {
                biased;

                Ok(cmd) = self.command_rx.recv() => match cmd {
                    Command::StopActor(ref id) if id == self.ctx.actor_id() => break,
                    Command::StopRuntime => break,
                    _ => {}
                },

                maybe_event = self.receiver.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };

                    #[cfg(feature = "monitoring")]
                    let topic = {
                        let topic = Arc::new(T::from_event(event.event()));
                        self.notify_event_delivered(&event, &topic);
                        topic
                    };

                    let res = self.actor.handle_event(&event).await;

                    #[cfg(feature = "monitoring")]
                    self.notify_event_handled(&event, &topic);

                    self.handle_error(res)?;

                    let mut cnt = 1;
                    while let Ok(event) = self.receiver.try_recv() {
                        #[cfg(feature = "monitoring")] self.notify_event_delivered(&event, &topic);
                        let res = self.actor.handle_event(&event).await;
                        #[cfg(feature = "monitoring")] self.notify_event_handled(&event, &topic);
                        self.handle_error(res)?;
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
    fn handle_error<R>(&self, result: Result<R>) -> Result<()> {
        if let Err(e) = result {
            #[cfg(feature = "monitoring")]
            self.notify_error(&e);

            self.actor.on_error(e)?;
        }
        Ok(())
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
