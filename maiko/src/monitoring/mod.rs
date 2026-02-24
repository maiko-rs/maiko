//! Monitoring API for observing event flow through the system.
//!
//! Enable with the `monitoring` feature:
//!
//! ```toml
//! [dependencies]
//! maiko = { version = "0.2", features = ["monitoring"] }
//! ```
//!
//! # Overview
//!
//! The monitoring system provides hooks into the event lifecycle:
//! - Event dispatched, delivered, and handled
//! - Actor errors and lifecycle events
//!
//! # Example
//!
//! ```ignore
//! use maiko::monitoring::Monitor;
//!
//! struct EventLogger;
//!
//! impl<E: Event, T: Topic<E>> Monitor<E, T> for EventLogger {
//!     fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
//!         println!("[handled] {} by {}", envelope.id(), receiver.as_str());
//!     }
//! }
//!
//! // Register with supervisor
//! let handle = sup.monitors().add(EventLogger).await;
//! ```

mod command;
mod dispatcher;
mod monitor;
mod monitor_handle;
mod monitoring_event;
mod registry;
mod sink;

/// Unique identifier for a registered monitor.
pub type MonitorId = u16;

pub(crate) use command::MonitorCommand;
pub(crate) use dispatcher::MonitorDispatcher;
pub use monitor::Monitor;
pub use monitor_handle::MonitorHandle;
pub(crate) use monitoring_event::MonitoringEvent;
pub use registry::MonitorRegistry;

pub(crate) use sink::MonitoringSink;
