use crate::{ActorId, DefaultTopic, Envelope, Event, OverflowPolicy, StepAction, Topic};

/// Trait for observing event flow through the system.
///
/// Implement this trait to receive callbacks during event lifecycle stages.
/// All methods have default no-op implementations, so you only need to
/// override the ones you care about.
///
/// # Example
///
/// ```rust
/// # use maiko::{ActorId, Envelope, Event, Topic};
/// use maiko::monitoring::Monitor;
///
/// struct EventLogger;
///
/// impl<E: Event, T: Topic<E>> Monitor<E, T> for EventLogger {
///     fn on_event_dispatched(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
///         println!("[dispatch] {} -> {}", envelope.meta().actor_name(), receiver.as_str());
///     }
///
///     fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
///         println!("[handled] {} by {}", envelope.id(), receiver.as_str());
///     }
/// }
/// ```
///
/// # Event Lifecycle
///
/// Events flow through these stages:
/// 1. **Dispatched**  - Broker routes event to a subscriber
/// 2. **Delivered**  - Actor receives event from its mailbox
/// 3. **Handled**  - Actor's `handle_event()` completes
///
/// For a single event delivered to N actors, each callback fires N times.
pub trait Monitor<E: Event, T: Topic<E> = DefaultTopic>: Send {
    /// Called when the broker dispatches an event to a subscriber.
    ///
    /// This fires once per subscriber that will receive the event.
    fn on_event_dispatched(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        let _e = envelope;
        let _t = topic;
        let _r = receiver;
    }

    /// Called when an actor receives an event from its mailbox.
    ///
    /// This fires just before `handle_event()` is called.
    fn on_event_delivered(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        let _e = envelope;
        let _t = topic;
        let _r = receiver;
    }

    /// Called after an actor finishes processing an event.
    ///
    /// This fires after `handle_event()` returns (success or error).
    fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        let _e = envelope;
        let _t = topic;
        let _r = receiver;
    }

    /// Called when a new actor is registered in the system.
    ///
    /// Fires once when the actor is spawned and added to the broker registry.
    fn on_actor_registered(&self, actor_id: &ActorId) {
        let _a = actor_id;
    }

    /// Called when an actor's handler returns an error.
    fn on_error(&self, err: &str, actor_id: &ActorId) {
        let _a = actor_id;
        let _e = err;
    }

    /// Called when an actor enters its `step()` method.
    fn on_step_enter(&self, actor_id: &ActorId) {
        let _a = actor_id;
    }

    /// Called when an actor exits its `step()` method.
    fn on_step_exit(&self, step_action: &StepAction, actor_id: &ActorId) {
        let _s = step_action;
        let _a = actor_id;
    }

    /// Called when a subscriber's channel is full and an overflow policy is triggered.
    ///
    /// Fires once per affected subscriber, before the policy action (drop, block,
    /// or channel close) takes effect. See [`OverflowPolicy`] for details.
    fn on_overflow(
        &self,
        envelope: &Envelope<E>,
        topic: &T,
        receiver: &ActorId,
        policy: OverflowPolicy,
    ) {
        let _e = envelope;
        let _t = topic;
        let _r = receiver;
        let _p = policy;
    }

    /// Called when an actor stops.
    fn on_actor_stop(&self, actor_id: &ActorId) {
        let _a = actor_id;
    }
}
