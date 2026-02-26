use crate::Config;

/// Per-actor configuration.
///
/// Controls settings that vary between actors, such as the mailbox channel
/// capacity. Created automatically by [`Supervisor::add_actor`] using global
/// defaults from [`Config`], or customized via [`ActorBuilder`].
///
/// # Examples
///
/// ```rust
/// use maiko::{Config, ActorConfig};
///
/// let config = ActorConfig::new(&Config::default())
///     .with_channel_capacity(512)
///     .with_max_events_per_tick(64);
///
/// assert_eq!(config.channel_capacity(), 512);
/// assert_eq!(config.max_events_per_tick(), 64);
/// ```
///
/// With the builder API (see [`ActorBuilder`](crate::ActorBuilder)):
///
/// ```rust,ignore
/// sup.build_actor("writer", |ctx| Writer::new(ctx))
///     .channel_capacity(512)
///     .topics(&[Topic::Data])
///     .build()?;
/// ```
///
/// [`Supervisor::add_actor`]: crate::Supervisor::add_actor
/// [`ActorBuilder`]: crate::ActorBuilder
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorConfig {
    channel_capacity: usize,
    max_events_per_tick: usize,
}

impl ActorConfig {
    /// Create a new config inheriting defaults from the global [`Config`].
    pub fn new(global_config: &Config) -> Self {
        Self {
            channel_capacity: global_config.default_actor_channel_capacity(),
            max_events_per_tick: global_config.default_max_events_per_tick(),
        }
    }

    /// Set the actor's mailbox channel capacity.
    ///
    /// This is the number of events that can be queued for this actor
    /// before the broker's overflow policy takes effect.
    pub fn with_channel_capacity(mut self, channel_capacity: usize) -> Self {
        self.channel_capacity = channel_capacity;
        self
    }

    /// Returns the actor's mailbox channel capacity.
    pub fn channel_capacity(&self) -> usize {
        self.channel_capacity
    }

    /// Set the maximum number of events processed per tick cycle.
    ///
    /// After processing this many events, the actor yields to allow other
    /// tasks to run and to call [`Actor::step`](crate::Actor::step).
    pub fn with_max_events_per_tick(mut self, max_events: usize) -> Self {
        self.max_events_per_tick = max_events;
        self
    }

    /// Returns the maximum number of events processed per tick cycle.
    pub fn max_events_per_tick(&self) -> usize {
        self.max_events_per_tick
    }
}

impl Default for ActorConfig {
    fn default() -> Self {
        ActorConfig::new(&Config::default())
    }
}
