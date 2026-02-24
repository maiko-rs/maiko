use std::sync::Arc;

use tokio::sync::mpsc::Receiver;

use crate::{
    Actor, ActorConfig, ActorId, Context, Envelope, Event, Result, Subscribe, Supervisor, Topic,
    internal::Subscription,
};

/// Builder for registering an actor with custom configuration.
///
/// Returned by [`Supervisor::build_actor`]. Use this when you need to override
/// per-actor settings such as channel capacity or when you want to separate
/// actor construction from topic subscription.
///
/// Defaults to no topic subscriptions and channel capacity inherited from the
/// global [`Config`](crate::Config).
///
/// # Examples
///
/// ```rust,ignore
/// // Custom channel capacity for a slow consumer
/// sup.build_actor("writer", |ctx| Writer::new(ctx))
///     .topics(&[Topic::Data])
///     .channel_capacity(512)
///     .build()?;
///
/// // Replace the entire actor config
/// sup.build_actor("fast", |ctx| Fast::new(ctx))
///     .topics(Subscribe::all())
///     .config(my_config)
///     .build()?;
/// ```
///
/// [`Supervisor::build_actor`]: crate::Supervisor::build_actor
pub struct ActorBuilder<'a, E: Event, T: Topic<E>, A: Actor<Event = E>> {
    supervisor: &'a mut Supervisor<E, T>,
    actor: A,
    ctx: Context<A::Event>,
    config: ActorConfig,
    topics: Subscription<T>,
    receiver: Receiver<Arc<Envelope<E>>>,
}

impl<'a, E: Event, T: Topic<E>, A: Actor<Event = E>> ActorBuilder<'a, E, T, A> {
    pub(crate) fn new(
        supervisor: &'a mut Supervisor<E, T>,
        actor: A,
        ctx: Context<A::Event>,
        receiver: Receiver<Arc<Envelope<E>>>,
    ) -> Self {
        let config = ActorConfig::new(supervisor.config());
        Self {
            supervisor,
            ctx,
            actor,
            config,
            topics: Subscription::None,
            receiver,
        }
    }

    /// Set the topics this actor subscribes to.
    ///
    /// Accepts anything that converts to [`Subscribe`]: a topic slice,
    /// [`Subscribe::all()`], or [`Subscribe::none()`].
    pub fn topics<S>(mut self, topics: S) -> Self
    where
        S: Into<Subscribe<E, T>>,
    {
        self.topics = topics.into().0;
        self
    }

    /// Replace the entire [`ActorConfig`] for this actor.
    pub fn config<C>(mut self, config: C) -> Self
    where
        C: Into<ActorConfig>,
    {
        self.config = config.into();
        self
    }

    /// Transform the current [`ActorConfig`] with a closure.
    ///
    /// Unlike [`config()`](Self::config) which replaces the entire config,
    /// this preserves inherited defaults and lets you tweak individual fields.
    ///
    /// ```rust,ignore
    /// sup.build_actor("consumer", |ctx| Consumer::new(ctx))
    ///     .topics(&[Topic::Data])
    ///     .channel_capacity(256)
    ///     .with_config(|c| c.with_max_events_per_tick(64))
    ///     .build()?;
    /// ```
    pub fn with_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(ActorConfig) -> ActorConfig,
    {
        self.config = f(self.config);
        self
    }

    /// Set the actor's mailbox channel capacity.
    ///
    /// Shorthand for modifying the channel capacity without replacing the
    /// full config. See [`ActorConfig::with_channel_capacity`].
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.config = self.config.with_channel_capacity(capacity);
        self
    }

    /// Register the actor with the supervisor and return its [`ActorId`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateActorName`](crate::Error::DuplicateActorName)
    /// if an actor with the same name is already registered.
    pub fn build(self) -> Result<ActorId> {
        self.supervisor.register_actor(
            self.ctx,
            self.actor,
            self.topics,
            self.config,
            self.receiver,
        )
    }
}
