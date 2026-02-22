//! Ready-to-use monitor implementations.
//!
//! This module contains concrete [`Monitor`](crate::monitoring::Monitor) implementations
//! for common use cases like event recording and logging.
//!
//! # Available Monitors
//!
//! - [`Tracer`] - Logs event lifecycle via `tracing` crate
//! - [`ActorMonitor`] - Tracks actor lifecycle and overflow counts
//! - [`Recorder`] - Records events to a JSON Lines file (requires `recorder` feature)
//!
//! # Example
//!
//! ```ignore
//! use maiko::monitors::Tracer;
//!
//! sup.monitors().add(Tracer).await;
//! ```

mod tracer;
pub use tracer::Tracer;

mod actor_monitor;
pub use actor_monitor::ActorMonitor;

#[cfg(feature = "recorder")]
mod recorder;

#[cfg(feature = "recorder")]
#[cfg_attr(docsrs, doc(cfg(feature = "recorder")))]
pub use recorder::Recorder;
