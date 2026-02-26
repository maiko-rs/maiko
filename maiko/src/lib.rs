#![cfg_attr(docsrs, feature(doc_cfg))]
//! # Maiko
//!
//! A topic-based pub/sub actor runtime for Tokio.
//!
//! Maiko helps you build multi-task Tokio applications without manually wiring
//! channels or spawning tasks. Declare actors and subscriptions, and Maiko handles
//! event routing and lifecycle management. Think Kafka-style pub/sub, but embedded
//! in your Tokio application.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use maiko::*;
//!
//! #[derive(Event, Clone, Debug)]
//! enum MyEvent {
//!     Hello(String),
//! }
//!
//! struct Greeter;
//!
//! impl Actor for Greeter {
//!     type Event = MyEvent;
//!
//!     async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result {
//!         if let MyEvent::Hello(name) = envelope.event() {
//!             println!("Hello, {}!", name);
//!         }
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result {
//!     let mut sup = Supervisor::<MyEvent>::default();
//!     sup.add_actor("greeter", |_ctx| Greeter, &[DefaultTopic])?;
//!
//!     sup.start().await?;
//!     sup.send(MyEvent::Hello("World".into())).await?;
//!     sup.stop().await
//! }
//! ```
//!
//! ## Core Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`Event`] | Marker trait for event types (use `#[derive(Event)]`) |
//! | [`Actor`] | Trait for implementing actors |
//! | [`Topic`] | Routes events to interested actors |
//! | [`Supervisor`] | Manages actor lifecycles and runtime |
//! | [`Context`] | Allows actors to send events and interact with runtime |
//! | [`Envelope`] | Wraps events with metadata (sender, timestamp, parent ID) |
//! | [`ActorId`] | Unique identifier for a registered actor |
//! | [`OverflowPolicy`] | Controls behavior when a subscriber's channel is full |
//!
//! ## Topic-Based Routing
//!
//! Actors subscribe to topics, and events are automatically routed to all interested subscribers.
//! Use [`DefaultTopic`] when you don't need routing, or implement [`Topic`] for custom filtering:
//!
//! ```rust,ignore
//! #[derive(Debug, Hash, Eq, PartialEq, Clone)]
//! enum MyTopic { Data, Control }
//!
//! impl Topic<MyEvent> for MyTopic {
//!     fn from_event(event: &MyEvent) -> Self {
//!         match event {
//!             MyEvent::Data(_) => MyTopic::Data,
//!             MyEvent::Control(_) => MyTopic::Control,
//!         }
//!     }
//! }
//!
//! sup.add_actor("processor", |ctx| Processor::new(ctx), &[MyTopic::Data])?;
//! ```
//!
//! ## Flow Control
//!
//! Events pass through two channel stages:
//!
//! 1. **Stage 1** (producer to broker) - per-actor channel, always blocks when full
//! 2. **Stage 2** (broker to subscriber) - per-actor channel, governed by [`OverflowPolicy`]
//!
//! Override [`Topic::overflow_policy()`] to control stage 2 behavior per topic:
//!
//! ```rust,ignore
//! fn overflow_policy(&self) -> OverflowPolicy {
//!     match self {
//!         MyTopic::Data    => OverflowPolicy::Block,  // wait for space
//!         MyTopic::Metrics => OverflowPolicy::Drop,   // discard if slow
//!     }
//! }
//! ```
//!
//! Producers can check [`Context::is_sender_full()`] to skip non-essential
//! sends when stage 1 is congested:
//!
//! ```rust,ignore
//! if !ctx.is_sender_full() {
//!     ctx.send(Event::Telemetry(stats)).await?;
//! }
//! ```
//!
//! See [`OverflowPolicy`] for details on each variant and trade-offs.
//!
//! ## Features
//!
//! - **`macros`** (default) - `#[derive(Event)]`, `#[derive(Label)]`, and `#[derive(SelfRouting)]` macros
//! - **`monitoring`** - Event lifecycle hooks for debugging, metrics, and logging
//! - **`test-harness`** - Test utilities for recording, spying, and asserting on event flow (enables `monitoring`)
//! - **`serde`** - JSON serialization support (e.g. `Supervisor::to_json()`)
//! - **`recorder`** - Built-in `Recorder` monitor for writing events to JSON Lines files (enables `monitoring` and `serde`)
//!
//! ## Examples
//!
//! See the [`examples/`](https://github.com/maiko-rs/maiko/tree/main/maiko/examples) directory:
//!
//! - `pingpong.rs`  - Simple event exchange between actors
//! - `guesser.rs`  - Multi-actor game with topics and timing
//! - `arbitrage.rs`  - Test harness demonstration

mod actor;
mod actor_builder;
mod actor_config;
mod actor_id;
mod context;
mod envelope;
mod error;
mod event;
mod event_id;
mod into_envelope;
mod label;
mod meta;
mod overflow_policy;
mod step_action;
mod subscribe;
mod supervisor;
mod supervisor_config;
mod topic;

mod internal;

#[cfg(feature = "test-harness")]
#[cfg_attr(docsrs, doc(cfg(feature = "test-harness")))]
pub mod testing;

#[cfg(feature = "monitoring")]
#[cfg_attr(docsrs, doc(cfg(feature = "monitoring")))]
pub mod monitoring;

#[cfg(feature = "monitoring")]
#[cfg_attr(docsrs, doc(cfg(feature = "monitoring")))]
pub mod monitors;

pub use actor::Actor;
pub use actor_builder::ActorBuilder;
pub use actor_config::ActorConfig;
pub use actor_id::ActorId;
pub use context::Context;
pub use envelope::Envelope;
pub use error::Error;
pub use event::Event;
pub use event_id::EventId;
pub use into_envelope::IntoEnvelope;
pub use label::Label;
pub use meta::Meta;
pub use overflow_policy::OverflowPolicy;
pub use step_action::StepAction;
pub use subscribe::Subscribe;
pub use supervisor::Supervisor;
pub use supervisor_config::SupervisorConfig;
pub use topic::{DefaultTopic, Topic};

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use maiko_macros::{Event, Label, SelfRouting};

/// Convenience alias for `Result<T, maiko::Error>`.
pub type Result<T = ()> = std::result::Result<T, Error>;
