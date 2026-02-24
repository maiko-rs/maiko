use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::{ActorId, Envelope, Event, OverflowPolicy, Topic, monitoring::Monitor};

/// Monitor that tracks actor lifecycle and overflow counts.
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
/// ```
#[derive(Clone)]
pub struct ActorMonitor {
    inner: Arc<Mutex<ActorMonitorInner>>,
}

struct ActorMonitorInner {
    active: HashSet<ActorId>,
    stopped: HashSet<ActorId>,
    overflow_counts: HashMap<ActorId, usize>,
}

impl ActorMonitor {
    /// Create a new `ActorMonitor`.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ActorMonitorInner {
                active: HashSet::new(),
                stopped: HashSet::new(),
                overflow_counts: HashMap::new(),
            })),
        }
    }

    /// Returns a snapshot of currently active actor IDs.
    pub fn actors(&self) -> Vec<ActorId> {
        let lock = self.inner.lock().unwrap();
        lock.active.iter().cloned().collect()
    }

    /// Returns a snapshot of stopped actor IDs.
    pub fn stopped_actors(&self) -> Vec<ActorId> {
        let lock = self.inner.lock().unwrap();
        lock.stopped.iter().cloned().collect()
    }

    /// Returns `true` if the actor is currently active.
    pub fn is_alive(&self, actor: &ActorId) -> bool {
        let lock = self.inner.lock().unwrap();
        lock.active.contains(actor)
    }

    /// Returns the number of overflow events observed for this actor.
    pub fn overflow_count(&self, actor: &ActorId) -> usize {
        let lock = self.inner.lock().unwrap();
        lock.overflow_counts.get(actor).copied().unwrap_or(0)
    }
}

impl<E, T> Monitor<E, T> for ActorMonitor
where
    E: Event,
    T: Topic<E> + Send,
{
    fn on_actor_registered(&self, actor_id: &ActorId) {
        let mut lock = self.inner.lock().unwrap();
        lock.active.insert(actor_id.clone());
        lock.stopped.remove(actor_id);
    }

    fn on_actor_stop(&self, actor_id: &ActorId) {
        let mut lock = self.inner.lock().unwrap();
        lock.active.remove(actor_id);
        lock.stopped.insert(actor_id.clone());
    }

    fn on_overflow(
        &self,
        _envelope: &Envelope<E>,
        _topic: &T,
        receiver: &ActorId,
        _policy: OverflowPolicy,
    ) {
        let mut lock = self.inner.lock().unwrap();
        *lock.overflow_counts.entry(receiver.clone()).or_insert(0) += 1;
    }
}

impl Default for ActorMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ActorMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lock = self.inner.lock().unwrap();
        f.debug_struct("ActorMonitor")
            .field("active", &lock.active.len())
            .field("stopped", &lock.stopped.len())
            .field("overflows", &lock.overflow_counts.len())
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
    }

    #[test]
    fn registered_actor_is_alive() {
        let monitor = ActorMonitor::new();
        let a = make_id("actor-1");
        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);

        assert!(monitor.is_alive(&a));
        assert!(monitor.actors().contains(&a));
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
    fn unknown_actor_is_not_alive() {
        let monitor = ActorMonitor::new();
        let a = make_id("unknown");
        assert!(!monitor.is_alive(&a));
        assert_eq!(monitor.overflow_count(&a), 0);
    }

    #[test]
    fn clone_shares_state() {
        let monitor = ActorMonitor::new();
        let query = monitor.clone();
        let a = make_id("actor-5");

        let m: &dyn Monitor<TestEvent, DefaultTopic> = &monitor;
        m.on_actor_registered(&a);

        assert!(query.is_alive(&a));
    }
}
