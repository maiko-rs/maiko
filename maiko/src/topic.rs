use std::hash::Hash;

use crate::{OverflowPolicy, event::Event};

/// Maps events to routing topics.
///
/// Implement this for your own topic type (usually an enum) to classify
/// events for the broker. Actors subscribe to one or more topics, and the
/// broker delivers events to matching subscribers.
///
/// Topics must be `Hash + Eq + Clone + Send + Sync + 'static` because they
/// are used as subscription keys in the broker, which runs in a spawned task.
///
/// Common patterns:
/// - Enum topics for simple classification.
/// - Struct topics when you need richer metadata (e.g., names or IDs).
///
/// Trait bounds: refer to the event trait as [`crate::Event`] in generic
/// signatures to avoid confusion with the `Event` derive macro.
pub trait Topic<E: Event>: Hash + PartialEq + Eq + Clone + Send + Sync + 'static {
    /// Classify an event into a topic.
    ///
    /// Called by the broker for every incoming event to determine which
    /// subscribers should receive it. Each subscriber registered for the
    /// returned topic gets a copy of the event.
    ///
    /// ```rust,ignore
    /// fn from_event(event: &MyEvent) -> Self {
    ///     match event {
    ///         MyEvent::Data(_) => MyTopic::Data,
    ///         MyEvent::Control(_) => MyTopic::Control,
    ///     }
    /// }
    /// ```
    fn from_event(event: &E) -> Self;

    /// Returns the overflow policy for this topic.
    ///
    /// Controls what the broker does when a subscriber's channel is full.
    /// See [`OverflowPolicy`] for details on each variant.
    ///
    /// The default is [`OverflowPolicy::Fail`], which closes the
    /// subscriber's channel on overflow. This ensures problems are
    /// surfaced immediately rather than hidden by silent drops.
    /// Override this method to set per-topic policies:
    ///
    /// ```rust,ignore
    /// fn overflow_policy(&self) -> OverflowPolicy {
    ///     match self {
    ///         MyTopic::Control => OverflowPolicy::Block,
    ///         MyTopic::Metrics => OverflowPolicy::Drop,
    ///     }
    /// }
    /// ```
    fn overflow_policy(&self) -> OverflowPolicy {
        OverflowPolicy::Fail
    }
}

/// Unit topic for systems that don't need topic-based routing.
///
/// Since every event maps to the same single topic, actors subscribing to
/// `DefaultTopic` will receive all events. This is the default generic
/// parameter on [`Supervisor`](crate::Supervisor), keeping simple setups
/// free of a custom topic type.
///
/// # Examples
///
/// ```rust, ignore
/// use maiko::{Supervisor, DefaultTopic};
/// let mut sup = Supervisor::<MyEvent>::default();
/// sup.add_actor("actor", |ctx| MyActor { ctx }, &[DefaultTopic])?;
/// ```
#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct DefaultTopic;

impl<E: Event> Topic<E> for DefaultTopic {
    fn from_event(_event: &E) -> DefaultTopic {
        DefaultTopic
    }
}

impl std::fmt::Display for DefaultTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;

    #[derive(Debug, Clone)]
    struct TestEvent;

    impl Event for TestEvent {}

    #[test]
    fn test_default_topic() {
        let event = TestEvent;
        let result = DefaultTopic::from_event(&event);
        assert_eq!(result.to_string(), "default");
    }

    #[test]
    fn test_topic_as_enum() {
        #[allow(dead_code)]
        #[derive(Clone)]
        enum TestEvent {
            Temperature(f64),
            Humidity(f64),
            Exit,
            Sleep(u64),
        }
        impl Event for TestEvent {}

        #[derive(Debug, PartialEq, Eq, Hash, Clone)]
        enum TestTopic {
            IoT,
            System,
        }
        impl Topic<TestEvent> for TestTopic {
            fn from_event(event: &TestEvent) -> Self {
                match event {
                    TestEvent::Temperature(_) | TestEvent::Humidity(_) => TestTopic::IoT,
                    TestEvent::Exit | TestEvent::Sleep(_) => TestTopic::System,
                }
            }
        }
        assert_eq!(
            TestTopic::from_event(&TestEvent::Temperature(22.5)),
            TestTopic::IoT
        );
        assert_eq!(TestTopic::from_event(&TestEvent::Exit), TestTopic::System);
        assert_eq!(
            TestTopic::from_event(&TestEvent::Sleep(1000)),
            TestTopic::System
        );
        assert_eq!(
            TestTopic::from_event(&TestEvent::Humidity(45.0)),
            TestTopic::IoT
        );
    }

    #[test]
    fn test_topic_as_struct() {
        #[allow(dead_code)]
        #[derive(Clone)]
        enum TestEvent {
            Temperature(f64),
            Humidity(f64),
            Exit,
            Sleep(u64),
        }
        impl Event for TestEvent {}

        #[derive(Debug, PartialEq, Eq, Hash, Clone)]
        struct TestTopic {
            name: String,
        }
        impl Topic<TestEvent> for TestTopic {
            fn from_event(event: &TestEvent) -> Self {
                match event {
                    TestEvent::Temperature(_) | TestEvent::Humidity(_) => TestTopic {
                        name: "IoT".to_string(),
                    },
                    TestEvent::Exit | TestEvent::Sleep(_) => TestTopic {
                        name: "System".to_string(),
                    },
                }
            }
        }

        assert_eq!(
            TestTopic::from_event(&TestEvent::Temperature(22.5)),
            TestTopic {
                name: "IoT".to_string()
            }
        );
        assert_eq!(
            TestTopic::from_event(&TestEvent::Exit),
            TestTopic {
                name: "System".to_string()
            }
        );
    }
}
