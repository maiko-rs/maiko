use std::sync::Arc;

use futures_util::{FutureExt, StreamExt, future::join_all, stream::SelectAll};
use tokio::{
    select,
    sync::{
        broadcast,
        mpsc::{Receiver, error::TrySendError},
    },
};
use tokio_stream::wrappers::ReceiverStream;

use super::Subscriber;
use crate::{ActorId, Envelope, Error, Event, OverflowPolicy, Result, Topic, internal::Command};

#[cfg(feature = "monitoring")]
use crate::monitoring::{MonitoringEvent, MonitoringSink};

type Payload<E> = Arc<Envelope<E>>;

pub struct Broker<E: Event, T: Topic<E>> {
    senders: SelectAll<ReceiverStream<Payload<E>>>,
    subscribers: Vec<Subscriber<E, T>>,
    command_tx: broadcast::Sender<Command>,

    #[cfg(feature = "monitoring")]
    monitoring: MonitoringSink<E, T>,
}

impl<E: Event, T: Topic<E>> Broker<E, T> {
    pub fn new(
        command_tx: broadcast::Sender<Command>,
        #[cfg(feature = "monitoring")] monitoring: MonitoringSink<E, T>,
    ) -> Broker<E, T> {
        Broker {
            senders: SelectAll::new(),
            subscribers: Vec::new(),
            command_tx,
            #[cfg(feature = "monitoring")]
            monitoring,
        }
    }

    pub(crate) fn add_subscriber(&mut self, subscriber: Subscriber<E, T>) -> Result<()> {
        if self.subscribers.contains(&subscriber) {
            return Err(Error::DuplicateActorName(subscriber.actor_id.clone()));
        }

        #[cfg(feature = "monitoring")]
        self.record_actor_registered(&subscriber.actor_id);

        self.subscribers.push(subscriber);

        Ok(())
    }

    pub(crate) fn add_sender(&mut self, receiver: Receiver<Payload<E>>) {
        self.senders.push(ReceiverStream::new(receiver));
    }

    async fn send_event(&mut self, e: &Arc<Envelope<E>>) -> Result {
        let topic = T::from_event(e.event());
        let mut blocked = None;

        #[cfg(feature = "monitoring")]
        let (is_recording, topic_for_monitor) = {
            let active = self.monitoring.is_active();
            let t = if active {
                Some(Arc::new(topic.clone()))
            } else {
                None
            };
            (active, t)
        };

        for subscriber in self
            .subscribers
            .iter()
            .filter(|s| s.topics.contains(&topic))
            .filter(|s| !s.is_closed())
            .filter(|s| s.actor_id != *e.meta().actor_id())
        {
            match subscriber.sender.try_send(e.clone()) {
                Ok(_) => {
                    #[cfg(feature = "monitoring")]
                    self.record_event_dispatched(
                        is_recording,
                        e,
                        &topic_for_monitor,
                        &subscriber.actor_id,
                    );
                }
                Err(TrySendError::Full(event)) => {
                    let policy = topic.overflow_policy();
                    #[cfg(feature = "monitoring")]
                    self.record_overflow(
                        is_recording,
                        e,
                        &topic_for_monitor,
                        &subscriber.actor_id,
                        policy,
                    );
                    match policy {
                        OverflowPolicy::Fail => {
                            self.command_tx
                                .send(Command::StopActor(subscriber.actor_id.clone()))?;
                            continue;
                        }
                        OverflowPolicy::Drop => {
                            continue;
                        }
                        OverflowPolicy::Block => {
                            let fut = subscriber.sender.send(event);
                            blocked.get_or_insert(Vec::new()).push(fut);
                        }
                    };
                }
                Err(TrySendError::Closed(_)) => {
                    // Channel is closed, will be cleaned up in the next maintenance cycle
                    tracing::warn!(actor=%subscriber.actor_id.as_str(), "subscriber channel closed, will be removed in cleanup");
                }
            }
        }

        if let Some(b) = blocked.take() {
            join_all(b).await;
        }

        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut command_rx = self.command_tx.subscribe();
        loop {
            select! {
                biased;
                Ok(cmd) = command_rx.recv() => match cmd {
                    Command::StopBroker => break,
                    Command::StopActor(id) => self.remove_subscriber(id),
                    _ => {}
                },
                Some(event) = self.senders.next() => {
                    self.send_event(&event).await?;
                },
            }
        }
        self.shutdown().await;
        Ok(())
    }

    fn remove_subscriber(&mut self, actor_id: ActorId) {
        self.subscribers.retain(|s| s.actor_id != actor_id);
    }

    async fn shutdown(&mut self) {
        use tokio::time::*;

        // Drain any events still buffered in sender streams (best effort)
        while let Some(event) = self.senders.next().now_or_never().flatten() {
            let _ = self.send_event(&event).await;
        }

        tokio::task::yield_now().await;

        // Wait the inner channels to be consumed by the actors
        let start = Instant::now();
        let timeout = Duration::from_millis(10);
        while !self.is_empty() && start.elapsed() < timeout {
            sleep(Duration::from_micros(100)).await;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.subscribers
            .iter()
            .all(|s| s.is_closed() || s.sender.capacity() == s.sender.max_capacity())
    }
}

#[cfg(feature = "monitoring")]
impl<E: Event, T: Topic<E>> Broker<E, T> {
    #[inline]
    fn record_event_dispatched(
        &self,
        is_recording: bool,
        e: &Arc<Envelope<E>>,
        topic: &Option<Arc<T>>,
        actor_id: &ActorId,
    ) {
        if is_recording {
            if let Some(topic_for_monitor) = topic {
                self.monitoring.send(MonitoringEvent::EventDispatched(
                    e.clone(),
                    topic_for_monitor.clone(),
                    actor_id.clone(),
                ));
            }
        }
    }

    #[inline]
    fn record_overflow(
        &self,
        is_recording: bool,
        e: &Arc<Envelope<E>>,
        topic: &Option<Arc<T>>,
        actor_id: &ActorId,
        policy: OverflowPolicy,
    ) {
        if is_recording {
            if let Some(topic_for_monitor) = topic {
                self.monitoring.send(MonitoringEvent::Overflow(
                    e.clone(),
                    topic_for_monitor.clone(),
                    actor_id.clone(),
                    policy,
                ));
            }
        }
    }

    fn record_actor_registered(&self, actor_id: &ActorId) {
        if self.monitoring.is_active() {
            self.monitoring
                .send(MonitoringEvent::ActorRegistered(actor_id.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Event, Topic, internal::Subscription, internal::broker::Broker};
    use std::{collections::HashSet, sync::Arc};
    use tokio::sync::{broadcast, mpsc};

    #[derive(Debug, Clone)]
    struct TestEvent {
        pub id: u32,
    }
    impl Event for TestEvent {}

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum TestTopic {
        A,
        B,
    }
    impl Topic<TestEvent> for TestTopic {
        fn from_event(event: &TestEvent) -> Self {
            if event.id % 2 == 0 {
                TestTopic::A
            } else {
                TestTopic::B
            }
        }
    }

    #[tokio::test]
    async fn test_add_subscriber() {
        use crate::ActorId;

        let (tx, rx) = mpsc::channel(10);
        let config = Arc::new(crate::Config::default());
        let (command_tx, _) = broadcast::channel(10);

        #[cfg(feature = "monitoring")]
        let monitoring = {
            let registry = crate::monitoring::MonitorRegistry::<TestEvent, TestTopic>::new(&config);
            registry.sink()
        };

        let mut broker = Broker::<TestEvent, TestTopic>::new(
            command_tx,
            #[cfg(feature = "monitoring")]
            monitoring,
        );
        broker.add_sender(rx);
        let actor_id = ActorId::new("subscriber1");
        let subscriber = super::Subscriber::new(
            actor_id.clone(),
            Subscription::Topics(HashSet::from([TestTopic::A])),
            tx.clone(),
        );
        assert!(broker.add_subscriber(subscriber).is_ok());
        let duplicate_subscriber = super::Subscriber::new(
            actor_id,
            Subscription::Topics(HashSet::from([TestTopic::B])),
            tx.clone(),
        );
        assert!(broker.add_subscriber(duplicate_subscriber).is_err());
    }
}
