//! Actor trace view for querying which actors were visited by an event chain.

use std::collections::HashSet;

use crate::{ActorId, Event, EventId, Topic};

use super::EventChain;

/// Actor trace view for querying which actors were visited by the chain.
#[derive(Debug)]
pub struct ActorTrace<'a, E: Event, T: Topic<E>> {
    pub(super) chain: &'a EventChain<E, T>,
}

impl<'a, E: Event, T: Topic<E>> ActorTrace<'a, E, T> {
    /// Returns all actors involved in this chain (sender and receivers).
    ///
    /// The list includes the root event's sender and all actors
    /// that received events in any branch. Order is not guaranteed.
    pub fn all(&self) -> Vec<&ActorId> {
        let mut actors: HashSet<&ActorId> =
            self.chain.chain_entries().map(|e| e.receiver()).collect();

        // Include root sender
        if let Some(sender) = self.chain.root_sender() {
            actors.insert(sender);
        }

        actors.into_iter().collect()
    }

    /// Returns true if all specified actors received events from this chain (any branch).
    pub fn visited(&self, actors: &[&ActorId]) -> bool {
        let all_actors: HashSet<_> = self.all().into_iter().collect();
        actors.iter().all(|a| all_actors.contains(*a))
    }

    /// Returns true if a path exactly matches the specified actors from root to leaf.
    ///
    /// This requires an exact match - every actor in the path must match.
    ///
    /// # Example
    ///
    /// For a chain with path `[Scanner, Pipeline, Writer, Telemetry]`:
    /// ```ignore
    /// chain.actors().exact(&[&scanner, &pipeline, &writer, &telemetry])  // true
    /// chain.actors().exact(&[&scanner, &pipeline, &writer])  // false - incomplete
    /// chain.actors().exact(&[&scanner, &telemetry])  // false - missing actors
    /// ```
    pub fn exact(&self, actors: &[&ActorId]) -> bool {
        if actors.is_empty() {
            return true;
        }

        let paths = self.paths();
        paths.iter().any(|path| {
            path.len() == actors.len() && path.iter().zip(actors.iter()).all(|(a, b)| *a == *b)
        })
    }

    /// Returns true if any path contains the specified actors as a contiguous subsequence.
    ///
    /// Unlike `exact`, this matches partial paths but requires no gaps between actors.
    ///
    /// # Example
    ///
    /// For a chain with path `[Scanner, Pipeline, Writer, Telemetry]`:
    /// ```ignore
    /// chain.actors().segment(&[&pipeline, &writer])  // true - contiguous
    /// chain.actors().segment(&[&scanner, &writer])  // false - gap (missing Pipeline)
    /// ```
    pub fn segment(&self, actors: &[&ActorId]) -> bool {
        if actors.is_empty() {
            return true;
        }

        let paths = self.paths();
        paths
            .iter()
            .any(|path| Self::contains_contiguous(path, actors))
    }

    /// Returns true if any path visits the specified actors in order (gaps allowed).
    ///
    /// Use this for sparse assertions where you care about ordering but not
    /// the intermediate actors.
    ///
    /// # Example
    ///
    /// For a chain with path `[Scanner, Pipeline, Writer, Telemetry]`:
    /// ```ignore
    /// chain.actors().passes_through(&[&scanner, &telemetry])  // true - in order with gaps
    /// chain.actors().passes_through(&[&telemetry, &scanner])  // false - wrong order
    /// ```
    pub fn passes_through(&self, actors: &[&ActorId]) -> bool {
        if actors.is_empty() {
            return true;
        }

        let paths = self.paths();
        paths
            .iter()
            .any(|path| Self::contains_subsequence(path, actors))
    }

    /// Returns the number of distinct paths in the event chain.
    ///
    /// Each path represents a branch from the root event to a leaf (an event
    /// with no children).
    pub fn path_count(&self) -> usize {
        self.paths().len()
    }

    /// Returns all distinct paths through the chain.
    ///
    /// Each path is a sequence of actors from the root sender to a leaf event's receiver.
    pub fn paths(&self) -> Vec<Vec<&'a ActorId>> {
        let mut paths = Vec::new();

        let Some(root_sender) = self.chain.root_sender() else {
            return paths;
        };

        // Start DFS from root
        self.build_paths_dfs(self.chain.root_id(), vec![root_sender], &mut paths);

        paths
    }

    /// DFS to build all paths from current event to leaves.
    fn build_paths_dfs(
        &self,
        event_id: EventId,
        current_path: Vec<&'a ActorId>,
        paths: &mut Vec<Vec<&'a ActorId>>,
    ) {
        // Find all receivers for this event (same event delivered to multiple receivers)
        let receivers: Vec<&ActorId> = self
            .chain
            .chain_entries()
            .filter(|e| e.id() == event_id)
            .map(|e| e.receiver())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Get children of this event
        let children = self.chain.children_of(event_id);

        if children.is_empty() {
            // Leaf node - each receiver creates a complete path
            for receiver in receivers {
                let mut path = current_path.clone();
                if path.last().copied() != Some(receiver) {
                    path.push(receiver);
                }
                paths.push(path);
            }
        } else {
            // For each child, find which receiver sent it and continue that path
            for child_id in &children {
                // Find who sent this child event
                if let Some(child_sender) = self.chain.sender_of(*child_id) {
                    let mut path = current_path.clone();
                    // Add the receiver who became the sender of the child
                    if path.last().copied() != Some(child_sender) {
                        path.push(child_sender);
                    }
                    self.build_paths_dfs(*child_id, path, paths);
                }
            }

            // Also add paths for receivers who didn't send children (leaf branches)
            let child_senders: HashSet<&ActorId> = children
                .iter()
                .filter_map(|&id| self.chain.sender_of(id))
                .collect();

            for receiver in receivers {
                if !child_senders.contains(receiver) {
                    // This receiver didn't continue the chain - it's a leaf
                    let mut path = current_path.clone();
                    if path.last().copied() != Some(receiver) {
                        path.push(receiver);
                    }
                    paths.push(path);
                }
            }
        }
    }

    /// Check if `path` contains `subsequence` in order (with gaps allowed).
    fn contains_subsequence(path: &[&ActorId], subsequence: &[&ActorId]) -> bool {
        let mut sub_iter = subsequence.iter();
        let mut current = sub_iter.next();

        for actor in path {
            if let Some(expected) = current {
                if *actor == *expected {
                    current = sub_iter.next();
                }
            }
        }

        current.is_none()
    }

    /// Check if `path` contains `subsequence` as a contiguous block (no gaps).
    fn contains_contiguous(path: &[&ActorId], subsequence: &[&ActorId]) -> bool {
        if subsequence.len() > path.len() {
            return false;
        }

        path.windows(subsequence.len())
            .any(|window| window.iter().zip(subsequence.iter()).all(|(a, b)| *a == *b))
    }
}
