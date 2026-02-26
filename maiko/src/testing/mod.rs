//! Test harness for observing and asserting on event flow.
//!
//! Enable with the `test-harness` feature:
//!
//! ```toml
//! [dev-dependencies]
//! maiko = { version = "0.2", features = ["test-harness"] }
//! ```
//!
//! # Example
//!
//! ```ignore
//! use maiko::testing::Harness;
//!
//! // Create harness BEFORE starting supervisor
//! let mut test = Harness::new(&mut supervisor).await;
//! supervisor.start().await?;
//!
//! test.record().await;
//! let id = test.send_as(&producer, MyEvent::Data(42)).await?;
//! test.settle().await;
//!
//! // Query using spies
//! assert!(test.event(id).was_delivered_to(&consumer));
//! assert_eq!(1, test.actor(&consumer).events_received());
//!
//! // Or use EventQuery for complex queries
//! let orders = test.events()
//!     .sent_by(&trader)
//!     .matching_event(|e| matches!(e, MyEvent::Order(_)))
//!     .count();
//!
//! // Or trace event propagation with EventChain
//! let chain = test.chain(id);
//! assert!(chain.actors().exact(&[&producer, &processor, &writer]));  // exact path
//! assert!(chain.actors().segment(&[&processor, &writer]));  // contiguous sub-path
//! assert!(chain.actors().passes_through(&[&producer, &writer]));  // gaps allowed
//! assert!(chain.events().segment(&["Input", "Processed", "Output"]));
//! ```
//!
//! # Note
//!
//! Spy and query types use `Rc` internally and are `!Send`. This is intentional —
//! they are designed for single-threaded test contexts only.
//!
//! # Warning
//!
//! **Do not use in production.** See [`Harness`] documentation for details.

mod actor_spy;
mod actor_trace;
mod event_chain;
mod event_collector;
mod event_entry;
mod event_matcher;
mod event_query;
mod event_spy;
mod event_trace;
pub(crate) mod expectation;
mod harness;
mod topic_spy;

pub use actor_spy::ActorSpy;
pub use actor_trace::ActorTrace;
pub use event_chain::EventChain;
pub(crate) use event_collector::EventCollector;
pub use event_entry::EventEntry;
pub use event_matcher::EventMatcher;
pub use event_query::EventQuery;
pub use event_spy::EventSpy;
pub use event_trace::EventTrace;
pub use expectation::Expectation;
pub use harness::Harness;
pub use topic_spy::TopicSpy;

pub(crate) type EventRecords<E, T> = std::sync::Arc<Vec<EventEntry<E, T>>>;
