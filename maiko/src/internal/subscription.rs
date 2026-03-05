use std::{collections::HashSet, hash::Hash};

/// Internal representation of topic subscriptions.
///
/// This is kept separate from `Subscribe` to allow the internal
/// representation to evolve without affecting the public API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subscription<T: Eq + Hash> {
    /// Subscribe to all topics (e.g., monitoring actors)
    All,
    /// Subscribe to specific topics
    Topics(HashSet<T>),
    /// Subscribe to no topics (e.g., pure event producers)
    None,
}
