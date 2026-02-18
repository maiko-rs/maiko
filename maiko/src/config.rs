/// Runtime configuration for the supervisor and actors.
///
/// Controls channel buffer sizes and event batching behavior. Use the builder
/// pattern to customize, or use [`Default`] for sensible defaults.
///
/// # Examples
///
/// ```rust
/// use maiko::Config;
///
/// let config = Config::default()
///     .with_broker_channel_capacity(512)          // Larger broker buffer
///     .with_default_actor_channel_capacity(256)   // Larger actor mailboxes
///     .with_default_max_events_per_tick(20);       // Process more events per cycle
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    /// Size of the broker's input channel buffer.
    /// Determines how many events can be queued before producers block (stage 1).
    /// Default: 256
    broker_channel_capacity: usize,

    /// Default mailbox channel capacity for newly registered actors.
    /// Individual actors can override this via [`ActorBuilder::channel_capacity`](crate::ActorBuilder::channel_capacity).
    /// Default: 128
    pub default_actor_channel_capacity: usize,

    /// Maximum number of events an actor will process in a single tick cycle
    /// before yielding control back to the scheduler.
    /// Lower values improve fairness, higher values improve throughput.
    /// Default: 10
    default_max_events_per_tick: usize,

    /// How often the broker cleans up closed subscriber channels.
    /// Default: 10s
    maintenance_interval: tokio::time::Duration,

    /// Buffer size for the monitoring event channel.
    /// Default: 1024
    monitoring_channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            broker_channel_capacity: 256,
            default_actor_channel_capacity: 128,
            default_max_events_per_tick: 10,
            maintenance_interval: tokio::time::Duration::from_secs(10),
            monitoring_channel_capacity: 1024,
        }
    }
}

impl Config {
    /// Set the per-actor channel capacity for stage 1 (actor to broker).
    pub fn with_broker_channel_capacity(mut self, capacity: usize) -> Self {
        self.broker_channel_capacity = capacity;
        self
    }

    /// Returns the per-actor channel capacity for stage 1 (actor to broker).
    pub fn broker_channel_capacity(&self) -> usize {
        self.broker_channel_capacity
    }

    /// Set the maximum number of events processed per tick cycle.
    ///
    /// This controls batching behavior in the actor event loop.
    /// After processing this many events, the actor yields to allow
    /// other tasks to run and to call [`Actor::step`].
    ///
    /// Trade-offs:
    /// - Lower values (1-5): Better fairness, more responsive `step()`, higher overhead
    /// - Higher values (50-100): Better throughput, potential starvation of `step()`
    ///
    /// [`Actor::step`]: crate::Actor::step
    /// Set the default maximum events processed per tick for new actors.
    pub fn with_default_max_events_per_tick(mut self, limit: usize) -> Self {
        self.default_max_events_per_tick = limit;
        self
    }

    /// Returns the default maximum events processed per tick.
    pub fn default_max_events_per_tick(&self) -> usize {
        self.default_max_events_per_tick
    }

    /// Set the maintenance interval for the broker.
    ///
    /// This controls how often the broker cleans up expired subscribers.
    pub fn with_maintenance_interval(mut self, interval: tokio::time::Duration) -> Self {
        self.maintenance_interval = interval;
        self
    }

    /// Returns the broker maintenance interval.
    pub fn maintenance_interval(&self) -> tokio::time::Duration {
        self.maintenance_interval
    }

    /// Set the buffer size for the monitoring event channel.
    pub fn with_monitoring_channel_capacity(mut self, capacity: usize) -> Self {
        self.monitoring_channel_capacity = capacity;
        self
    }

    /// Returns the monitoring event channel capacity.
    pub fn monitoring_channel_capacity(&self) -> usize {
        self.monitoring_channel_capacity
    }

    /// Set the default mailbox channel capacity for new actors (stage 2).
    pub fn with_default_actor_channel_capacity(mut self, capacity: usize) -> Self {
        self.default_actor_channel_capacity = capacity;
        self
    }

    /// Returns the default mailbox channel capacity for new actors.
    pub fn default_actor_channel_capacity(&self) -> usize {
        self.default_actor_channel_capacity
    }
}
