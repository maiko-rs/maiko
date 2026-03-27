use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;

use crate::{ActorId, Envelope, Event, OverflowPolicy, StepAction, Topic, monitoring::Monitor};

/// Best-effort runtime state derived from lifecycle and `step()` callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorState {
    /// Actor is registered/running without a more specific step-derived state.
    Active,
    /// Actor is currently executing `step()`.
    Stepping,
    /// Actor returned `StepAction::AwaitEvent`.
    AwaitingEvent,
    /// Actor returned `StepAction::Backoff`.
    BackingOff(Duration),
    /// Actor returned `StepAction::Never`.
    StepDisabled,
    /// Actor stop was observed.
    Stopped,
}

/// Monitor that tracks actor lifecycle and per-actor event flow metrics.
///
/// Register with the supervisor to passively observe actor registration,
/// shutdown, and overflow events. Query at any time from any thread.
///
/// ```ignore
/// let monitor = ActorMonitor::new();
/// let query = monitor.clone();
/// sup.monitors().add(monitor).await;
///
/// // Later, from any thread:
/// let alive = query.is_alive(&actor_id);
/// let overflows = query.overflow_count(&actor_id);
/// let dispatched = query.dispatched_count(&actor_id);
/// let delivered = query.delivered_count(&actor_id);
/// let handled = query.handled_count(&actor_id);
/// let errors = query.error_count(&actor_id);
/// let depth = query.queue_depth(&actor_id);
/// let state = query.state(&actor_id);
/// ```
#[derive(Clone)]
pub struct ActorMonitor {
    actors: Arc<DashMap<ActorId, RwLock<ActorStats>>>,
}

struct ActorStats {
    stopped: bool,
    step_status: StepStatus,
    dispatched_count: usize,
    delivered_count: usize,
    handled_count: usize,
    overflow_count: usize,
    error_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepStatus {
    /// No `step()` activity has been observed yet.
    None,
    /// The actor is currently executing `step()`.
    InStep,
    /// The most recent completed `step()` returned this action.
    Last(StepAction),
}

impl ActorStats {
    fn new() -> Self {
        Self {
            stopped: false,
            step_status: StepStatus::None,
            dispatched_count: 0,
            delivered_count: 0,
            handled_count: 0,
            overflow_count: 0,
            error_count: 0,
        }
    }

    fn state(&self) -> ActorState {
        if self.stopped {
            ActorState::Stopped
        } else {
            match self.step_status {
                StepStatus::None => ActorState::Active,
                StepStatus::InStep => ActorState::Stepping,
                StepStatus::Last(StepAction::AwaitEvent) => ActorState::AwaitingEvent,
                StepStatus::Last(StepAction::Backoff(duration)) => ActorState::BackingOff(duration),
                StepStatus::Last(StepAction::Never) => ActorState::StepDisabled,
                StepStatus::Last(StepAction::Continue | StepAction::Yield) => ActorState::Active,
            }
        }
    }
}

impl ActorMonitor {
    /// Create a new `ActorMonitor`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            actors: Arc::new(DashMap::new()),
        }
    }

    /// Returns a snapshot of currently active actor IDs.
    pub fn actors(&self) -> Vec<ActorId> {
        self.actors
            .iter()
            .filter_map(|entry| {
                let stats = entry.value().read().unwrap();
                (!stats.stopped).then(|| entry.key().clone())
            })
            .collect()
    }

    /// Returns a snapshot of stopped actor IDs.
    pub fn stopped_actors(&self) -> Vec<ActorId> {
        self.actors
            .iter()
            .filter_map(|entry| {
                let stats = entry.value().read().unwrap();
                stats.stopped.then(|| entry.key().clone())
            })
            .collect()
    }

    /// Returns `true` if the actor is currently active.
    pub fn is_alive(&self, actor: &ActorId) -> bool {
        self.actors
            .get(actor)
            .map(|entry| !entry.value().read().unwrap().stopped)
            .unwrap_or(false)
    }

    /// Returns `true` if the actor was registered and has since stopped.
    ///
    /// Returns `false` for actors that were never registered or are still active.
    pub fn is_stopped(&self, actor: &ActorId) -> bool {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().stopped)
            .unwrap_or(false)
    }

    /// Returns the number of overflow events observed for this actor.
    pub fn overflow_count(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().overflow_count)
            .unwrap_or(0)
    }

    /// Returns the number of dispatches observed for this actor.
    pub fn dispatched_count(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().dispatched_count)
            .unwrap_or(0)
    }

    /// Returns the number of deliveries observed for this actor.
    pub fn delivered_count(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().delivered_count)
            .unwrap_or(0)
    }

    /// Returns the number of handled events observed for this actor.
    pub fn handled_count(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().handled_count)
            .unwrap_or(0)
    }

    /// Returns the number of handler errors observed for this actor.
    pub fn error_count(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| entry.value().read().unwrap().error_count)
            .unwrap_or(0)
    }

    /// Returns the estimated queue depth for this actor.
    ///
    /// This is derived as `dispatched_count - delivered_count` and reflects
    /// monitor-observed backlog, not authoritative mailbox state.
    pub fn queue_depth(&self, actor: &ActorId) -> usize {
        self.actors
            .get(actor)
            .map(|entry| {
                let stats = entry.value().read().unwrap();
                stats.dispatched_count.saturating_sub(stats.delivered_count)
            })
            .unwrap_or(0)
    }

    /// Returns the best-effort runtime state for this actor.
    pub fn state(&self, actor: &ActorId) -> Option<ActorState> {
        self.actors.get(actor).map(|entry| {
            let stats = entry.value().read().unwrap();
            stats.state()
        })
    }
}

impl<E, T> Monitor<E, T> for ActorMonitor
where
    E: Event,
    T: Topic<E> + Send,
{
    fn on_event_dispatched(&self, _envelope: &Envelope<E>, _topic: &T, receiver: &ActorId) {
        let entry = self
            .actors
            .entry(receiver.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.dispatched_count += 1;
    }

    fn on_event_delivered(&self, _envelope: &Envelope<E>, _topic: &T, receiver: &ActorId) {
        let entry = self
            .actors
            .entry(receiver.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.delivered_count += 1;
    }

    fn on_event_handled(&self, _envelope: &Envelope<E>, _topic: &T, receiver: &ActorId) {
        let entry = self
            .actors
            .entry(receiver.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.handled_count += 1;
    }

    fn on_actor_registered(&self, actor_id: &ActorId) {
        let entry = self
            .actors
            .entry(actor_id.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.stopped = false;
    }

    fn on_actor_stop(&self, actor_id: &ActorId) {
        let entry = self
            .actors
            .entry(actor_id.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.stopped = true;
    }

    fn on_overflow(
        &self,
        _envelope: &Envelope<E>,
        _topic: &T,
        receiver: &ActorId,
        _policy: OverflowPolicy,
    ) {
        let entry = self
            .actors
            .entry(receiver.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.overflow_count += 1;
    }

    fn on_error(&self, _err: &str, actor_id: &ActorId) {
        let entry = self
            .actors
            .entry(actor_id.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.error_count += 1;
    }

    fn on_step_enter(&self, actor_id: &ActorId) {
        let entry = self
            .actors
            .entry(actor_id.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.step_status = StepStatus::InStep;
    }

    fn on_step_exit(&self, step_action: &StepAction, actor_id: &ActorId) {
        let entry = self
            .actors
            .entry(actor_id.clone())
            .or_insert_with(|| RwLock::new(ActorStats::new()));
        let mut stats = entry.write().unwrap();
        stats.step_status = StepStatus::Last(*step_action);
    }
}

impl Default for ActorMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ActorMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = self.actors().len();
        let stopped = self.stopped_actors().len();
        let overflows = self
            .actors
            .iter()
            .filter(|entry| entry.value().read().unwrap().overflow_count > 0)
            .count();
        f.debug_struct("ActorMonitor")
            .field("active", &active)
            .field("stopped", &stopped)
            .field("overflows", &overflows)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::DefaultTopic;

    use super::*;

    #[derive(Clone, Debug)]
    struct TestEvent;
    impl Event for TestEvent {}

    fn make_id(name: &str) -> ActorId {
        ActorId::new(name)
    }

    #[test]
    fn default_is_empty() {
        let m = ActorMonitor::default();
        assert!(m.actors().is_empty());
        assert!(m.stopped_actors().is_empty());
        assert_eq!(m.state(&make_id("missing")), None);
    }

    #[test]
    fn registered_actor_is_alive() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-1");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);

        assert!(monitor.is_alive(&a));
        assert!(monitor.actors().contains(&a));
        assert_eq!(monitor.state(&a), Some(ActorState::Active));
    }

    #[test]
    fn stopped_actor_is_not_alive() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-2");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);
        m.on_actor_stop(&a);

        assert!(!monitor.is_alive(&a));
        assert!(monitor.stopped_actors().contains(&a));
        assert_eq!(monitor.state(&a), Some(ActorState::Stopped));
    }

    #[test]
    fn overflow_count_increments() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-3");
        let env = Envelope::new(TestEvent, a.clone());
        let topic = DefaultTopic;

        assert_eq!(monitor.overflow_count(&a), 0);
        monitor.on_overflow(&env, &topic, &a, OverflowPolicy::Fail);
        assert_eq!(monitor.overflow_count(&a), 1);
        monitor.on_overflow(&env, &topic, &a, OverflowPolicy::Fail);
        assert_eq!(monitor.overflow_count(&a), 2);
    }

    #[test]
    fn overflow_and_lifecycle_are_independent() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-4");
        let env = Envelope::new(TestEvent, a.clone());
        let topic = DefaultTopic;

        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);
        monitor.on_overflow(&env, &topic, &a, OverflowPolicy::Fail);

        assert!(monitor.is_alive(&a));
        assert_eq!(monitor.overflow_count(&a), 1);

        m.on_actor_stop(&a);
        assert!(!monitor.is_alive(&a));
        assert_eq!(monitor.overflow_count(&a), 1);
    }

    #[test]
    fn flow_metrics_are_counted() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-5");
        let env = Envelope::new(TestEvent, a.clone());
        let topic = DefaultTopic;
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;

        m.on_event_dispatched(&env, &topic, &a);
        m.on_event_dispatched(&env, &topic, &a);
        m.on_event_delivered(&env, &topic, &a);
        m.on_event_handled(&env, &topic, &a);
        m.on_error("boom", &a);
        m.on_error("boom", &a);

        assert_eq!(monitor.dispatched_count(&a), 2);
        assert_eq!(monitor.delivered_count(&a), 1);
        assert_eq!(monitor.handled_count(&a), 1);
        assert_eq!(monitor.error_count(&a), 2);
        assert_eq!(monitor.overflow_count(&a), 0);
        assert_eq!(monitor.queue_depth(&a), 1);
    }

    #[test]
    fn queue_depth_saturates_at_zero() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-6");
        let env = Envelope::new(TestEvent, a.clone());
        let topic = DefaultTopic;
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;

        m.on_event_delivered(&env, &topic, &a);
        m.on_event_handled(&env, &topic, &a);

        assert_eq!(monitor.dispatched_count(&a), 0);
        assert_eq!(monitor.delivered_count(&a), 1);
        assert_eq!(monitor.queue_depth(&a), 0);
        assert_eq!(monitor.state(&a), Some(ActorState::Active));
    }

    #[test]
    fn unknown_actor_is_not_alive() {
        let monitor = ActorMonitor::new();
        let a = make_id("unknown");
        assert!(!monitor.is_alive(&a));
        assert_eq!(monitor.overflow_count(&a), 0);
        assert_eq!(monitor.dispatched_count(&a), 0);
        assert_eq!(monitor.delivered_count(&a), 0);
        assert_eq!(monitor.handled_count(&a), 0);
        assert_eq!(monitor.error_count(&a), 0);
        assert_eq!(monitor.queue_depth(&a), 0);
        assert_eq!(monitor.state(&a), None);
    }

    #[test]
    fn clone_shares_state() {
        let monitor = ActorMonitor::new();
        let query = monitor.clone();
        let a = make_id("actor-7");

        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);
        m.on_error("boom", &a);

        assert!(query.is_alive(&a));
        assert_eq!(query.error_count(&a), 1);
    }

    #[test]
    fn state_tracks_step_lifecycle() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-8");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;

        m.on_actor_registered(&a);
        assert_eq!(monitor.state(&a), Some(ActorState::Active));

        m.on_step_enter(&a);
        assert_eq!(monitor.state(&a), Some(ActorState::Stepping));

        m.on_step_exit(&StepAction::AwaitEvent, &a);
        assert_eq!(monitor.state(&a), Some(ActorState::AwaitingEvent));

        m.on_step_enter(&a);
        m.on_step_exit(&StepAction::Backoff(Duration::from_millis(5)), &a);
        assert_eq!(
            monitor.state(&a),
            Some(ActorState::BackingOff(Duration::from_millis(5)))
        );

        m.on_step_enter(&a);
        m.on_step_exit(&StepAction::Never, &a);
        assert_eq!(monitor.state(&a), Some(ActorState::StepDisabled));

        m.on_step_enter(&a);
        m.on_step_exit(&StepAction::Continue, &a);
        assert_eq!(monitor.state(&a), Some(ActorState::Active));

        m.on_step_enter(&a);
        m.on_step_exit(&StepAction::Yield, &a);
        assert_eq!(monitor.state(&a), Some(ActorState::Active));
    }

    #[test]
    fn stopped_state_overrides_last_step_action() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-9");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;

        m.on_actor_registered(&a);
        m.on_step_enter(&a);
        m.on_step_exit(&StepAction::AwaitEvent, &a);
        m.on_actor_stop(&a);

        assert_eq!(monitor.state(&a), Some(ActorState::Stopped));
    }
}
