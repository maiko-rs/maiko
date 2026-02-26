use std::fmt;

/// Controls what happens when a subscriber's channel is full.
///
/// Events flow through two stages: producers send to the broker (stage 1),
/// and the broker dispatches to subscriber channels (stage 2). Overflow
/// policy governs stage 2 - what the broker does when a specific subscriber
/// can't keep up.
///
/// The policy is defined on [`Topic`](crate::Topic), not on actors, because
/// flow semantics are properties of the data: a critical command topic
/// needs different handling than expendable telemetry, regardless of which
/// actor consumes it.
///
/// # Policies
///
/// | Policy | On full channel | Use case |
/// |--------|----------------|----------|
/// | [`Drop`](Self::Drop) | Discard the event, continue | Telemetry, metrics, status updates |
/// | [`Block`](Self::Block) | Wait for space | Commands, data that must arrive |
/// | [`Fail`](Self::Fail) | Close the subscriber's channel | Real-time topics where stale data is worse than no actor |
///
/// # Default
///
/// The default policy is `Fail`, which ensures overflow is never silent.
/// If a subscriber can't keep up, the system surfaces the problem
/// immediately rather than hiding it behind dropped events. Override
/// [`Topic::overflow_policy()`](crate::Topic::overflow_policy)
/// to choose per-topic behavior.
///
/// # Single-broker limitation
///
/// With a single broker (current architecture), `Block` on one topic
/// delays dispatch to all other topics while the broker waits for space.
/// For most systems (tens to hundreds of actors, moderate event rates)
/// this delay is acceptable. Multi-broker support (planned) eliminates
/// this by giving each topic group its own broker.
///
/// # Example
///
/// ```rust
/// # use maiko::*;
/// # #[derive(Clone, Debug, Event)]
/// # enum MyEvent { Control, Telemetry }
/// # #[derive(Debug, Hash, Eq, PartialEq, Clone)]
/// # enum MyTopic { Control, Telemetry }
/// impl Topic<MyEvent> for MyTopic {
///     fn from_event(event: &MyEvent) -> Self {
///         match event {
///             MyEvent::Control => MyTopic::Control,
///             MyEvent::Telemetry => MyTopic::Telemetry,
///         }
///     }
///
///     fn overflow_policy(&self) -> OverflowPolicy {
///         match self {
///             MyTopic::Control => OverflowPolicy::Block,
///             MyTopic::Telemetry => OverflowPolicy::Drop,
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OverflowPolicy {
    /// Close the subscriber's channel, terminating the actor.
    ///
    /// The broker removes the subscriber immediately. The actor's
    /// receive channel closes, causing it to exit its event loop
    /// and run `on_shutdown()`. Use this for topics where falling
    /// behind is unacceptable and the actor should be stopped rather
    /// than allowed to process stale data.
    #[default]
    Fail,

    /// Discard the event when the subscriber's channel is full.
    ///
    /// The broker logs the drop and continues dispatching to other
    /// subscribers. The slow consumer never sees the event. Use this
    /// for data that is acceptable to lose under load (telemetry,
    /// periodic status updates, metrics).
    Drop,

    /// Wait for space in the subscriber's channel.
    ///
    /// The broker blocks (via `.await`) until the subscriber has capacity.
    /// Multiple blocked subscribers are awaited concurrently. Use this for
    /// events that must be delivered (commands, data integrity).
    ///
    /// With a single broker, this delays dispatch of all other topics
    /// during the wait.
    Block,
}

impl OverflowPolicy {
    /// Returns `true` if this is the [`Fail`](Self::Fail) policy.
    pub fn is_fail(&self) -> bool {
        matches!(self, OverflowPolicy::Fail)
    }

    /// Returns `true` if this is the [`Drop`](Self::Drop) policy.
    pub fn is_drop(&self) -> bool {
        matches!(self, OverflowPolicy::Drop)
    }

    /// Returns `true` if this is the [`Block`](Self::Block) policy.
    pub fn is_block(&self) -> bool {
        matches!(self, OverflowPolicy::Block)
    }
}

impl fmt::Display for OverflowPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverflowPolicy::Fail => write!(f, "Fail"),
            OverflowPolicy::Drop => write!(f, "Drop"),
            OverflowPolicy::Block => write!(f, "Block"),
        }
    }
}
