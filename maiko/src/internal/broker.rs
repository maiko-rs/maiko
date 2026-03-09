use std::{collections::HashMap, sync::Arc};

use super::Subscriber;
use crate::{
    ActorId, Envelope, Error, Event, OverflowPolicy, Result, Topic,
    internal::{Command, CommandSender, Subscription},
};
use futures_util::{StreamExt, future::join_all, stream::SelectAll};
use tokio::{
    select,
    sync::{
        broadcast,
        mpsc::{Receiver, error::TrySendError},
    },
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

#[cfg(feature = "monitoring")]
use crate::monitoring::{MonitoringEvent, MonitoringSink};

type Payload<E> = Arc<Envelope<E>>;

pub struct Broker<E: Event, T: Topic<E>> {
    senders: SelectAll<ReceiverStream<Payload<E>>>,

    /// Canonical subscriber registry keyed by [`ActorId`].
    subscribers: HashMap<ActorId, Subscriber<E, T>>,
    /// Index of explicit topic subscriptions ([`Subscription::Topics`]).
    subscribers_by_topic: HashMap<T, Vec<ActorId>>,
    /// [`ActorId`]s subscribed to all topics ([`Subscription::All`]).
    subscribers_for_all: Vec<ActorId>,

    command_tx: CommandSender,
    command_rx: broadcast::Receiver<Command>,

    #[cfg(feature = "monitoring")]
    monitoring: MonitoringSink<E, T>,
}

impl<E: Event, T: Topic<E>> Broker<E, T> {
    pub fn new(
        command_tx: CommandSender,
        #[cfg(feature = "monitoring")] monitoring: MonitoringSink<E, T>,
    ) -> Broker<E, T> {
        Broker {
            senders: SelectAll::new(),
            subscribers: HashMap::new(),
            subscribers_by_topic: HashMap::new(),
            subscribers_for_all: Vec::new(),
            command_rx: command_tx.as_ref().subscribe(),
            command_tx,
            #[cfg(feature = "monitoring")]
            monitoring,
        }
    }

    pub(crate) fn add_subscriber(&mut self, subscriber: Subscriber<E, T>) -> Result {
        if self.subscribers.contains_key(&subscriber.actor_id) {
            return Err(Error::DuplicateActorName(subscriber.actor_id.clone()));
        }

        #[cfg(feature = "monitoring")]
        self.record_actor_registered(&subscriber.actor_id);

        let actor_id = subscriber.actor_id.clone();

        match &subscriber.topics {
            Subscription::All => self.subscribers_for_all.push(actor_id.clone()),
            Subscription::Topics(topics) => {
                for topic in topics {
                    self.subscribers_by_topic
                        .entry(topic.clone())
                        .or_default()
                        .push(actor_id.clone());
                }
            }
            Subscription::None => {}
        }

        self.subscribers.insert(actor_id, subscriber);

        Ok(())
    }

    pub(crate) fn add_sender(&mut self, receiver: Receiver<Payload<E>>) {
        self.senders.push(ReceiverStream::new(receiver));
    }

    async fn send_event(&self, e: &Arc<Envelope<E>>) -> Result {
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

        let subscribers = self
            .subscribers_by_topic
            .get(&topic)
            .into_iter()
            .flatten()
            .chain(self.subscribers_for_all.iter())
            .filter_map(|actor_id| {
                let subscriber = self.subscribers.get(actor_id);
                if subscriber.is_none() {
                    // Generally, this must not happen.
                    // If it does, the indexing behaviour is flawed.
                    debug_assert!(subscriber.is_some(), "Indexing is flawed. Actor id {actor_id} is indexed, but there is no corresponding subscriber");
                    warn!("Actor id {actor_id} is indexed, but there is no corresponding subscriber");
                }
                subscriber
            })
            .filter(|s| !s.is_closed())
            .filter(|s| s.actor_id != *e.meta().actor_id());

        for subscriber in subscribers {
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
                    // Channel is closed — subscriber already stopped
                    tracing::warn!(actor=%subscriber.actor_id.as_str(), "subscriber channel closed");
                }
            }
        }

        if let Some(b) = blocked.take() {
            join_all(b).await;
        }

        Ok(())
    }

    pub async fn run(&mut self) -> Result {
        let mut res = Ok(());
        loop {
            select! {
                biased;
                cmd_res = self.command_rx.recv() => match cmd_res {
                    Ok(res) => match res {
                        Command::StopBroker => break,
                        Command::StopActor(id) => self.remove_subscriber(id),
                        _ => {}
                    }
                    Err(e) => {
                        res = Err(Error::internal(e));
                        break;
                    }
                },
                maybe_event = self.senders.next() => {
                    let Some(event) = maybe_event else {
                        break;
                    };
                    self.send_event(&event).await?;
                },
            }
        }
        self.shutdown().await;
        res
    }

    fn remove_subscriber(&mut self, actor_id: ActorId) {
        let Some(subscriber) = self.subscribers.remove(&actor_id) else {
            return;
        };

        match subscriber.topics {
            Subscription::All => {
                self.subscribers_for_all.retain(|id| id != &actor_id);
            }
            Subscription::Topics(topics) => {
                for topic in topics {
                    let mut should_remove_topic = false;
                    if let Some(actor_ids) = self.subscribers_by_topic.get_mut(&topic) {
                        actor_ids.retain(|id| id != &actor_id);
                        should_remove_topic = actor_ids.is_empty();
                    }

                    if should_remove_topic {
                        self.subscribers_by_topic.remove(&topic);
                    }
                }
            }
            Subscription::None => {}
        }
    }

    /// Stop accepting new Stage-1 messages while preserving buffered items.
    ///
    /// Stage naming follows crate-level [Flow Control](crate#flow-control).
    fn close_senders(&mut self) {
        for sender in self.senders.iter_mut() {
            sender.close();
        }
    }

    /// Gracefully stop broker ingress and drain all Stage-1 channels.
    ///
    /// Stage naming follows the crate-level
    /// [Flow Control](crate#flow-control) documentation.
    ///
    /// Shutdown first closes all Stage-1 receivers so no new messages can be
    /// enqueued. It then awaits `senders.next()` until `None`, which indicates
    /// every Stage-1 channel is closed and fully drained. Each drained event is
    /// still dispatched to subscribers to preserve in-flight delivery.
    async fn shutdown(&mut self) {
        self.close_senders();

        // Drain all Stage-1 channels until each receiver is closed and empty.
        while let Some(event) = self.senders.next().await {
            let _ = self.send_event(&event).await;
        }
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
    use crate::{
        Event, Topic,
        internal::{CommandSender, Subscription, broker::Broker},
    };
    use std::collections::HashSet;
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
        let (command_tx, _) = broadcast::channel(10);

        #[cfg(feature = "monitoring")]
        let monitoring = {
            let registry = crate::monitoring::MonitorRegistry::<TestEvent, TestTopic>::new(
                &crate::SupervisorConfig::default(),
            );
            registry.sink()
        };

        let mut broker = Broker::<TestEvent, TestTopic>::new(
            CommandSender::from(command_tx),
            #[cfg(feature = "monitoring")]
            monitoring,
        );
        broker.add_sender(rx);
        let actor_id_topic = ActorId::new("subscriber-topic");
        let topic_subscriber = super::Subscriber::new(
            actor_id_topic.clone(),
            Subscription::Topics(HashSet::from([TestTopic::A])),
            tx.clone(),
        );
        assert!(broker.add_subscriber(topic_subscriber).is_ok());
        assert_eq!(broker.subscribers.len(), 1);
        assert_eq!(
            broker.subscribers_by_topic.get(&TestTopic::A),
            Some(&vec![actor_id_topic.clone()])
        );
        assert!(!broker.subscribers_by_topic.contains_key(&TestTopic::B));
        assert!(broker.subscribers_for_all.is_empty());

        let actor_id_all = ActorId::new("subscriber-all");
        let all_subscriber =
            super::Subscriber::new(actor_id_all.clone(), Subscription::All, tx.clone());
        assert!(broker.add_subscriber(all_subscriber).is_ok());
        assert_eq!(broker.subscribers.len(), 2);
        assert_eq!(broker.subscribers_for_all, vec![actor_id_all.clone()]);

        let actor_id_none = ActorId::new("subscriber-none");
        let none_subscriber =
            super::Subscriber::new(actor_id_none.clone(), Subscription::None, tx.clone());
        assert!(broker.add_subscriber(none_subscriber).is_ok());
        assert_eq!(broker.subscribers.len(), 3);
        assert_eq!(
            broker.subscribers_by_topic.get(&TestTopic::A),
            Some(&vec![actor_id_topic.clone()])
        );
        assert_eq!(broker.subscribers_for_all, vec![actor_id_all.clone()]);

        let duplicate_subscriber = super::Subscriber::new(
            actor_id_topic.clone(),
            Subscription::Topics(HashSet::from([TestTopic::B])),
            tx.clone(),
        );
        assert!(broker.add_subscriber(duplicate_subscriber).is_err());

        broker.remove_subscriber(actor_id_all.clone());
        assert!(broker.subscribers_for_all.is_empty());
        assert!(!broker.subscribers.contains_key(&actor_id_all));

        broker.remove_subscriber(actor_id_topic.clone());
        assert!(!broker.subscribers_by_topic.contains_key(&TestTopic::A));
        assert!(!broker.subscribers.contains_key(&actor_id_topic));

        broker.remove_subscriber(actor_id_none.clone());
        assert!(!broker.subscribers.contains_key(&actor_id_none));
        assert!(broker.subscribers.is_empty());
    }
}
