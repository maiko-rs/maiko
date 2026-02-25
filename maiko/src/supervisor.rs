use std::sync::Arc;

use tokio::{
    select,
    sync::{
        Mutex, Notify, broadcast,
        mpsc::{self, Receiver, Sender, channel},
    },
    task::JoinSet,
};

use crate::{
    Actor, ActorBuilder, ActorConfig, ActorId, Config, Context, DefaultTopic, Envelope,
    EnvelopeBuilder, Error, Event, Label, Result, Subscribe, Topic,
    internal::{ActorController, Broker, Command, Subscriber, Subscription},
};

#[cfg(feature = "monitoring")]
use crate::monitoring::MonitorRegistry;

/// Coordinates actors and the broker, and owns the top-level runtime.
///
/// # Actor Registration
///
/// ```ignore
/// // Subscribe to specific topics
/// supervisor.add_actor("processor", |ctx| Processor::new(ctx), &[MyTopic::Data])?;
///
/// // Subscribe to all topics (e.g., monitoring)
/// supervisor.add_actor("monitor", |ctx| Monitor::new(ctx), Subscribe::all())?;
///
/// // Subscribe to no topics (pure event producer)
/// supervisor.add_actor("producer", |ctx| Producer::new(ctx), Subscribe::none())?;
/// ```
///
/// # Runtime Control
///
/// - [`start()`](Self::start) spawns the broker loop and returns immediately (non-blocking).
/// - [`send(event)`](Self::send) emits events into the broker.
/// - [`run()`](Self::run) combines `start()` and `join()` — consumes the supervisor.
/// - [`join()`](Self::join) awaits all actor tasks to finish — consumes the supervisor.
/// - [`stop()`](Self::stop) graceful shutdown — consumes the supervisor.
///
/// The terminal methods (`run`, `join`, `stop`) take ownership of the supervisor,
/// preventing use-after-shutdown at compile time.
///
/// See also: [`Actor`], [`Context`], [`Topic`].
pub struct Supervisor<E: Event, T: Topic<E> = DefaultTopic> {
    config: Arc<Config>,
    broker: Arc<Mutex<Broker<E, T>>>,
    pub(crate) sender: Sender<Arc<Envelope<E>>>,
    tasks: JoinSet<Result<()>>,
    start_notifier: Arc<Notify>,
    supervisor_id: ActorId,
    registrations: Vec<(ActorId, Subscription<T>)>,

    cmd_sender: broadcast::Sender<Command>,

    #[cfg(feature = "monitoring")]
    monitoring: MonitorRegistry<E, T>,
}

impl<E: Event, T: Topic<E>> Supervisor<E, T> {
    /// Create a new supervisor with the given runtime configuration.
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let (tx, rx) = channel::<Arc<Envelope<E>>>(config.broker_channel_capacity());

        #[cfg(feature = "monitoring")]
        let monitoring = {
            let mut monitoring = MonitorRegistry::new(&config);
            monitoring.start();
            monitoring
        };

        let supervisor_id = ActorId::new("supervisor");

        let (cmd_sender, _) = broadcast::channel(32);

        let mut broker = Broker::new(
            cmd_sender.clone(),
            #[cfg(feature = "monitoring")]
            monitoring.sink(),
        );
        broker.add_sender(rx);

        Self {
            broker: Arc::new(Mutex::new(broker)),
            config,
            sender: tx,
            tasks: JoinSet::new(),
            start_notifier: Arc::new(Notify::new()),
            supervisor_id,
            registrations: Vec::new(),
            cmd_sender,

            #[cfg(feature = "monitoring")]
            monitoring,
        }
    }

    /// Register a new actor with a factory that receives a [`Context<E>`].
    ///
    /// This is the primary way to register actors with the supervisor.
    ///
    /// # Arguments
    ///
    /// * `name` - Actor identifier used for metadata and routing
    /// * `factory` - Closure that receives a Context and returns the actor
    /// * `topics` - Slice of topics the actor subscribes to
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateActorName`] if an actor with the same name
    /// is already registered. Returns [`Error::BrokerAlreadyStarted`] if
    /// called after [`start()`](Self::start).
    ///
    /// # Example
    ///
    /// ```ignore
    /// supervisor.add_actor(
    ///     "processor",
    ///     |ctx| DataProcessor::new(ctx),
    ///     &[MyTopic::Data, MyTopic::Control]
    /// )?;
    /// ```
    pub fn add_actor<A, F, S>(&mut self, name: &str, factory: F, topics: S) -> Result<ActorId>
    where
        A: Actor<Event = E>,
        F: FnOnce(Context<E>) -> A,
        S: Into<Subscribe<E, T>>,
    {
        let (tx, rx) = mpsc::channel::<Arc<Envelope<E>>>(self.config.broker_channel_capacity());
        let ctx = self.create_context(name, tx);
        let actor = factory(ctx.clone());
        let topics = topics.into().0;
        self.register_actor(ctx, actor, topics, ActorConfig::new(&self.config), rx)
    }

    /// Start building an actor registration with custom configuration.
    ///
    /// Returns an [`ActorBuilder`] that lets you set topics, channel capacity,
    /// or a full [`ActorConfig`] before calling [`build()`](ActorBuilder::build).
    ///
    /// Use this instead of [`add_actor`](Self::add_actor) when you need
    /// per-actor settings that differ from the global defaults.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// sup.build_actor("consumer", |ctx| Consumer::new(ctx))
    ///     .topics(&[Topic::Data, Topic::Command])
    ///     .channel_capacity(512)
    ///     .build()?;
    /// ```
    pub fn build_actor<'a, A, F>(&'a mut self, name: &str, factory: F) -> ActorBuilder<'a, E, T, A>
    where
        A: Actor<Event = E>,
        F: FnOnce(Context<E>) -> A,
    {
        let (tx, rx) = mpsc::channel::<Arc<Envelope<E>>>(self.config.broker_channel_capacity());
        let ctx = self.create_context(name, tx);
        let actor = factory(ctx.clone());
        ActorBuilder::new(self, actor, ctx, rx)
    }

    /// Internal method to register an actor with the supervisor.
    ///
    /// Called by `add_actor()` to perform the actual registration. It:
    /// 1. Creates a Subscriber and registers it with the broker
    /// 2. Creates an ActorHandler wrapping the actor
    /// 3. Spawns the actor task (which waits for start notification)
    pub(crate) fn register_actor<A>(
        &mut self,
        ctx: Context<E>,
        actor: A,
        topics: Subscription<T>,
        config: ActorConfig,
        receiver: Receiver<Arc<Envelope<E>>>,
    ) -> Result<ActorId>
    where
        A: Actor<Event = E>,
    {
        let actor_id = ctx.actor_id().clone();

        let mut broker = self
            .broker
            .try_lock()
            .map_err(|_| Error::BrokerAlreadyStarted)?;

        let (tx, rx) = mpsc::channel::<Arc<Envelope<E>>>(config.channel_capacity());

        let subscriber = Subscriber::<E, T>::new(actor_id.clone(), topics.clone(), tx);
        broker.add_subscriber(subscriber)?;
        broker.add_sender(receiver);
        self.registrations.push((actor_id.clone(), topics));

        let mut controller = ActorController::<A, T> {
            actor,
            receiver: rx,
            ctx,
            max_events_per_tick: config.max_events_per_tick(),
            command_rx: self.cmd_sender.subscribe(),

            #[cfg(feature = "monitoring")]
            monitoring: self.monitoring.sink(),

            _topic: std::marker::PhantomData,
        };

        let notified = self.start_notifier.clone().notified_owned();
        self.tasks.spawn(async move {
            notified.await;
            controller.run().await
        });

        Ok(actor_id)
    }

    /// Create a new Context for an actor.
    ///
    /// Internal helper used by `add_actor` to create actor contexts.
    pub(crate) fn create_context(
        &self,
        name: &str,
        sender: Sender<Arc<Envelope<E>>>,
    ) -> Context<E> {
        Context::<E>::new(ActorId::new(name), sender, self.cmd_sender.clone())
    }

    /// Emit an event into the broker from the supervisor.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SendError`] if the broker channel is closed.
    pub async fn send<B: Into<EnvelopeBuilder<E>>>(&self, builder: B) -> Result<()> {
        let envelope = builder
            .into()
            .with_actor_id(self.supervisor_id.clone())
            .build()?;
        self.sender.send(envelope.into()).await?;
        Ok(())
    }

    /// Convenience method to start and then await completion of all tasks.
    /// Blocks until shutdown.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`start()`](Self::start) or [`join()`](Self::join).
    pub async fn run(mut self) -> Result<()> {
        self.start().await?;
        self.join().await
    }

    /// Start the broker loop in a background task. This returns immediately.
    ///
    /// # Errors
    ///
    /// Currently infallible, but returns `Result` for forward compatibility.
    pub async fn start(&mut self) -> Result<()> {
        let broker = self.broker.clone();
        self.tasks
            .spawn(async move { broker.lock().await.run().await });
        self.start_notifier.notify_waiters();
        Ok(())
    }

    /// Waits until at least one of the actor tasks completes then
    /// triggers a shutdown if not already requested.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ActorJoinError`] if an actor task panics.
    /// Propagates any error returned by [`stop()`](Self::stop).
    pub async fn join(mut self) -> Result<()> {
        let mut cmd_rx = self.cmd_sender.subscribe();
        loop {
            select! {
                Ok(cmd) = cmd_rx.recv() => if cmd == Command::StopRuntime { break; },
                maybe_res = self.tasks.join_next() => {
                    match maybe_res {
                        Some(res) => res??,
                        None => break
                    }
                }
            }
        }
        self.stop().await?;
        Ok(())
    }

    /// Request a graceful shutdown, then await all actor tasks.
    ///
    /// # Shutdown Process
    ///
    /// 1. Waits for the broker to receive all pending events (up to 10 ms)
    /// 2. Sends `StopBroker` command and waits for the broker to drain actor queues
    /// 3. Sends `StopRuntime` command and waits for all actor tasks to complete
    ///
    /// # Errors
    ///
    /// Returns [`Error::ActorJoinError`] if an actor task panics during shutdown.
    pub async fn stop(mut self) -> Result<()> {
        use tokio::time::*;
        let start = Instant::now();
        let timeout = Duration::from_millis(10);
        let max = self.sender.max_capacity();

        // 1. Wait for the main channel to drain
        println!("1");
        while start.elapsed() < timeout {
            if self.sender.capacity() == max {
                break;
            }
            sleep(Duration::from_micros(100)).await;
        }
        println!("2");

        // 2. Wait for the broker to shutdown gracefully
        self.cmd_sender.send(Command::StopBroker)?;
        let _ = self.broker.lock().await;

        println!("3");
        // 3. Stop the actors
        self.cmd_sender.send(Command::StopRuntime)?;
        while let Some(res) = self.tasks.join_next().await {
            res??;
        }

        println!("4");
        #[cfg(feature = "monitoring")]
        self.monitoring.stop().await;

        Ok(())
    }

    /// Returns the supervisor's configuration.
    pub fn config(&self) -> &Config {
        self.config.as_ref()
    }

    #[cfg(feature = "monitoring")]
    #[cfg_attr(docsrs, doc(cfg(feature = "monitoring")))]
    pub fn monitors(&mut self) -> &mut MonitorRegistry<E, T> {
        &mut self.monitoring
    }
}

impl<E: Event, T: Topic<E>> Default for Supervisor<E, T> {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl<E: Event, T: Topic<E> + Label> Supervisor<E, T> {
    /// Generate a Mermaid flowchart showing actor subscriptions.
    ///
    /// Topics are shown as circles, actors as boxes. Arrows indicate
    /// that an actor subscribes to (receives events from) a topic.
    ///
    /// Actors with `Subscribe::all()` are connected to all known topics.
    /// Actors with `Subscribe::none()` appear isolated (no incoming arrows).
    ///
    /// # Example output
    ///
    /// ```text
    /// flowchart LR
    ///     SensorData((SensorData)) --> processor
    ///     SensorData --> logger
    ///     Alert((Alert)) --> logger
    /// ```
    ///
    /// Topic names are obtained via `Topic::name()`.
    pub fn to_mermaid(&self) -> String {
        let all_topics = self.all_topic_labels();

        let mut lines = vec!["flowchart LR".to_string()];

        // For each registration, add edges from topics to actors
        for (actor_id, subscription) in &self.registrations {
            let actor_name = actor_id.as_str();
            match subscription {
                Subscription::All => {
                    for topic_name in &all_topics {
                        lines.push(format!("    {}(({0})) --> {}", topic_name, actor_name));
                    }
                }
                Subscription::Topics(topics) => {
                    for topic in topics {
                        let topic_name = topic.label();
                        lines.push(format!("    {}(({0})) --> {}", topic_name, actor_name));
                    }
                }
                Subscription::None => {
                    // Pure producer - no incoming edges, but show the node
                    lines.push(format!("    {}[{}]", actor_name, actor_name));
                }
            }
        }

        lines.join("\n")
    }

    /// Collect all known topic labels from explicit subscriptions, sorted alphabetically.
    fn all_topic_labels(&self) -> Vec<String> {
        use std::collections::BTreeSet;

        let mut labels = BTreeSet::new();
        for (_, subscription) in &self.registrations {
            if let Subscription::Topics(topics) = subscription {
                for topic in topics {
                    labels.insert(topic.label().into_owned());
                }
            }
        }
        labels.into_iter().collect()
    }
}

impl<E: Event, T: Topic<E>> Drop for Supervisor<E, T> {
    fn drop(&mut self) {
        if self.cmd_sender.receiver_count() > 0 {
            let _ = self.cmd_sender.send(Command::StopRuntime);
            let _ = self.cmd_sender.send(Command::StopBroker);
        }
        #[cfg(feature = "monitoring")]
        self.monitoring.cancel();
    }
}

#[cfg(feature = "serde")]
impl<E: Event, T: Topic<E> + Label> Supervisor<E, T> {
    /// Export actor subscription topology as JSON.
    ///
    /// This method provides a machine-readable representation of which actors
    /// are subscribed to which topics. It mirrors the information shown by
    /// [`to_mermaid`](Self::to_mermaid), but returns structured JSON suitable
    /// for inspection, tooling, or testing.
    ///
    /// The output is a flat list where each entry contains:
    ///
    /// - `actor_id` - the actor name
    /// - `subscriptions` - topic labels the actor receives events from
    ///
    /// # Semantics
    ///
    /// - Actors registered with [`Subscribe::all()`] are expanded to include
    ///   all known topics discovered from explicit subscriptions.
    /// - Actors registered with [`Subscribe::none()`] produce an empty list.
    /// - Topic names are obtained via [`Label::label()`].
    ///
    /// This export reflects declared routing configuration only. It does not
    /// represent runtime message flow, event producers, or supervision
    /// hierarchy.
    ///
    /// # Errors
    ///
    /// Returns any serialization error produced by `serde_json`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let json = supervisor.to_json()?;
    /// println!("{json}");
    /// ```
    #[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
    pub fn to_json(&self) -> serde_json::Result<String> {
        use serde::Serialize;

        #[derive(Serialize)]
        struct ActorSubscriptionExport {
            actor_id: String,
            subscriptions: Vec<String>,
        }

        let all_topics = self.all_topic_labels();

        let mut exports = Vec::with_capacity(self.registrations.len());

        for (actor_id, subscription) in &self.registrations {
            let mut subs: Vec<String> = match subscription {
                Subscription::All => all_topics.clone(),
                Subscription::Topics(topics) => topics.iter().map(|t| t.label().into()).collect(),
                Subscription::None => Vec::new(),
            };
            subs.sort();

            exports.push(ActorSubscriptionExport {
                actor_id: actor_id.to_string(),
                subscriptions: subs,
            });
        }

        serde_json::to_string_pretty(&exports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Sensor(f64),
        Alert(String),
    }

    impl Event for TestEvent {}

    #[derive(Debug, Hash, Eq, PartialEq, Clone)]
    enum TestTopic {
        SensorData,
        Alerts,
    }

    impl Topic<TestEvent> for TestTopic {
        fn from_event(event: &TestEvent) -> Self {
            match event {
                TestEvent::Sensor(_) => TestTopic::SensorData,
                TestEvent::Alert(_) => TestTopic::Alerts,
            }
        }
    }

    impl Label for TestTopic {
        fn label(&self) -> std::borrow::Cow<'static, str> {
            std::borrow::Cow::Borrowed(match self {
                TestTopic::SensorData => "SensorData",
                TestTopic::Alerts => "Alerts",
            })
        }
    }

    struct DummyActor;

    impl Actor for DummyActor {
        type Event = TestEvent;
        async fn handle_event(&mut self, _: &Envelope<Self::Event>) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_to_mermaid_basic() {
        let mut sup = Supervisor::<TestEvent, TestTopic>::default();
        sup.add_actor("sensor", |_| DummyActor, Subscribe::none())
            .unwrap();
        sup.add_actor("processor", |_| DummyActor, &[TestTopic::SensorData])
            .unwrap();
        sup.add_actor("alerter", |_| DummyActor, &[TestTopic::Alerts])
            .unwrap();

        let mermaid = sup.to_mermaid();
        assert!(mermaid.starts_with("flowchart LR"));
        assert!(mermaid.contains("sensor[sensor]")); // producer node
        assert!(mermaid.contains("SensorData((SensorData)) --> processor"));
        assert!(mermaid.contains("Alerts((Alerts)) --> alerter"));
    }

    #[tokio::test]
    async fn test_to_mermaid_subscribe_all() {
        let mut sup = Supervisor::<TestEvent, TestTopic>::default();
        sup.add_actor("processor", |_| DummyActor, &[TestTopic::SensorData])
            .unwrap();
        sup.add_actor("alerter", |_| DummyActor, &[TestTopic::Alerts])
            .unwrap();
        sup.add_actor("monitor", |_| DummyActor, Subscribe::all())
            .unwrap();

        let mermaid = sup.to_mermaid();

        assert!(mermaid.contains("--> monitor"));

        let monitor_lines: Vec<_> = mermaid.lines().filter(|l| l.contains("monitor")).collect();
        assert_eq!(monitor_lines.len(), 2);
    }

    #[cfg(feature = "serde")]
    #[tokio::test]
    async fn test_to_json_basic() {
        use serde_json::Value;

        let mut sup = Supervisor::<TestEvent, TestTopic>::default();

        sup.add_actor("sensor", |_| DummyActor, Subscribe::none())
            .unwrap();

        sup.add_actor("processor", |_| DummyActor, &[TestTopic::SensorData])
            .unwrap();

        let json = sup.to_json().unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();

        let arr = parsed.as_array().unwrap();

        let sensor = arr.iter().find(|v| v["actor_id"] == "sensor").unwrap();

        let processor = arr.iter().find(|v| v["actor_id"] == "processor").unwrap();

        assert!(sensor["subscriptions"].as_array().unwrap().is_empty());

        let subs = processor["subscriptions"].as_array().unwrap();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], "SensorData");
    }

    #[cfg(feature = "serde")]
    #[tokio::test]
    async fn test_to_json_subscribe_all() {
        use serde_json::Value;

        let mut sup = Supervisor::<TestEvent, TestTopic>::default();

        sup.add_actor("processor", |_| DummyActor, &[TestTopic::SensorData])
            .unwrap();

        sup.add_actor("alerter", |_| DummyActor, &[TestTopic::Alerts])
            .unwrap();

        sup.add_actor("monitor", |_| DummyActor, Subscribe::all())
            .unwrap();

        let json = sup.to_json().unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();

        let arr = parsed.as_array().unwrap();

        let monitor = arr.iter().find(|v| v["actor_id"] == "monitor").unwrap();

        let subs = monitor["subscriptions"].as_array().unwrap();

        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&Value::String("SensorData".into())));
        assert!(subs.contains(&Value::String("Alerts".into())));
    }

    #[cfg(feature = "serde")]
    #[tokio::test]
    async fn test_to_json_is_valid_json() {
        let mut sup = Supervisor::<TestEvent, TestTopic>::default();
        sup.add_actor("a", |_| DummyActor, Subscribe::none())
            .unwrap();

        let json = sup.to_json().unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
    }
}
