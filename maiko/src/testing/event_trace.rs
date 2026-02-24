//! Event trace view for querying the sequence of events in a chain.

use std::collections::HashSet;

use crate::{Event, Label, Topic};

use super::{EventChain, EventEntry, EventMatcher};

/// Event trace view for querying the sequence of events in the chain.
#[derive(Debug)]
pub struct EventTrace<'a, E: Event, T: Topic<E>> {
    pub(super) chain: &'a EventChain<E, T>,
}

impl<E: Event + Label, T: Topic<E>> EventTrace<'_, E, T> {
    /// Returns all unique events in this chain (unordered).
    pub fn all(&self) -> Vec<&EventEntry<E, T>> {
        let mut seen_ids = HashSet::new();
        self.chain
            .chain_entries()
            .filter(|e| seen_ids.insert(e.id()))
            .collect()
    }

    /// Returns events in order of occurrence (BFS from root).
    ///
    /// Each unique event appears once, in the order it was reached
    /// during chain traversal.
    pub fn ordered(&self) -> Vec<&EventEntry<E, T>> {
        let mut seen_ids = HashSet::new();
        self.chain
            .ordered_entries()
            .into_iter()
            .filter(|e| seen_ids.insert(e.id()))
            .collect()
    }

    /// Returns true if the chain contains an event matching the given matcher.
    pub fn contains(&self, matcher: impl Into<EventMatcher<E, T>>) -> bool {
        let matcher = matcher.into();
        self.chain.chain_entries().any(|e| matcher.matches(e))
    }

    /// Returns true if any event path exactly matches all matchers (same length and order).
    ///
    /// Each path follows the parent-child tree from root to leaf. For branching
    /// chains, returns true if any single branch matches.
    pub fn exact<M>(&self, matchers: &[M]) -> bool
    where
        M: Into<EventMatcher<E, T>> + Clone,
    {
        if matchers.is_empty() {
            return true;
        }

        let matchers: Vec<_> = matchers.iter().cloned().map(|m| m.into()).collect();
        let paths = self.chain.event_paths();
        paths.iter().any(|path| {
            path.len() == matchers.len()
                && path
                    .iter()
                    .zip(matchers.iter())
                    .all(|(entry, matcher)| matcher.matches(entry))
        })
    }

    /// Returns true if events matching the matchers appear consecutively in any event path.
    ///
    /// Each path follows the parent-child tree from root to leaf. For branching
    /// chains, returns true if any single branch contains the contiguous segment.
    pub fn segment<M>(&self, matchers: &[M]) -> bool
    where
        M: Into<EventMatcher<E, T>> + Clone,
    {
        if matchers.is_empty() {
            return true;
        }

        let matchers: Vec<_> = matchers.iter().cloned().map(|m| m.into()).collect();
        let paths = self.chain.event_paths();
        paths
            .iter()
            .any(|path| Self::contains_contiguous(path, &matchers))
    }

    /// Returns true if events matching the matchers appear in order in any event path (gaps allowed).
    ///
    /// Each path follows the parent-child tree from root to leaf. For branching
    /// chains, returns true if any single branch passes through the matchers.
    pub fn passes_through<M>(&self, matchers: &[M]) -> bool
    where
        M: Into<EventMatcher<E, T>> + Clone,
    {
        if matchers.is_empty() {
            return true;
        }

        let matchers: Vec<_> = matchers.iter().cloned().map(|m| m.into()).collect();
        let paths = self.chain.event_paths();
        paths
            .iter()
            .any(|path| Self::contains_subsequence(path, &matchers))
    }

    /// Returns the number of distinct event paths in the chain.
    pub fn path_count(&self) -> usize {
        self.chain.event_paths().len()
    }

    fn contains_contiguous(path: &[&EventEntry<E, T>], matchers: &[EventMatcher<E, T>]) -> bool {
        if matchers.len() > path.len() {
            return false;
        }
        path.windows(matchers.len()).any(|window| {
            window
                .iter()
                .zip(matchers.iter())
                .all(|(entry, matcher)| matcher.matches(entry))
        })
    }

    fn contains_subsequence(path: &[&EventEntry<E, T>], matchers: &[EventMatcher<E, T>]) -> bool {
        let mut matcher_idx = 0;
        for entry in path {
            if matcher_idx >= matchers.len() {
                break;
            }
            if matchers[matcher_idx].matches(entry) {
                matcher_idx += 1;
            }
        }
        matcher_idx == matchers.len()
    }
}
