use std::collections::HashSet;

use crate::{Topic, internal::Subscription};

/// Specifies which topics an actor subscribes to.
///
/// Use the static constructors to create subscriptions:
///
/// - [`Subscribe::all()`]  - receive events on all topics (e.g., monitoring actors)
/// - [`Subscribe::none()`]  - receive no events (e.g., pure event producers)
/// - [`Subscribe::to`] - receive events on specific topics
///
/// For convenience, `&[T]` and `[T]` converts to `Subscribe` automatically:
///
/// ```ignore
/// // These are equivalent:
/// sup.add_actor("a", factory, &[Topic::Data])?;
/// sup.add_actor("a", factory, Subscribe::to([Topic::Data]))?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscribe<T: Topic>(pub(crate) Subscription<T>);

impl<T: Topic> Subscribe<T> {
    /// Subscribe to all topics.
    ///
    /// Useful for monitoring actors that need to observe all events.
    pub fn all() -> Self {
        Subscribe(Subscription::All)
    }

    /// Subscribe to no topics.
    ///
    /// Useful for actors that only produce events and don't need to receive any.
    pub fn none() -> Self {
        Subscribe(Subscription::None)
    }

    /// Subscribe to specific topics.
    ///
    /// Accepts any iterator of topics:
    /// ```ignore
    /// Subscribe::to([Topic::A, Topic::B])
    /// Subscribe::to(vec![Topic::A])
    /// Subscribe::to(&[Topic::A, Topic::B])
    /// ```
    pub fn to(topics: impl IntoIterator<Item = T>) -> Self {
        let set = HashSet::from_iter(topics);
        Subscribe(Subscription::Topics(set))
    }
}

impl<T: Topic> From<&[T]> for Subscribe<T> {
    fn from(topics: &[T]) -> Self {
        Subscribe::to(topics.iter().cloned())
    }
}

impl<T: Topic, const N: usize> From<[T; N]> for Subscribe<T> {
    fn from(topics: [T; N]) -> Self {
        Subscribe::to(topics)
    }
}

impl<T: Topic, const N: usize> From<&[T; N]> for Subscribe<T> {
    fn from(topics: &[T; N]) -> Self {
        Subscribe::to(topics.iter().cloned())
    }
}

impl<T: Topic> From<T> for Subscribe<T> {
    fn from(topic: T) -> Self {
        Subscribe::to([topic])
    }
}
