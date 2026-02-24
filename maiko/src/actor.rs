use core::marker::Send;
use std::future::Future;

use crate::{Envelope, Error, Event, Result, StepAction};

/// Core trait implemented by user-defined actors.
///
/// Actors are independent units that encapsulate state and process events sequentially.
/// Each actor has its own mailbox (channel) and processes one event at a time,
/// eliminating the need for locks or shared state synchronization.
///
/// # Core Methods
///
/// - [`handle_event`](Self::handle_event)  - Process incoming events (reactive)
/// - [`step`](Self::step)  - Perform periodic work or produce events (proactive)
///
/// # Lifecycle Hooks
///
/// - [`on_start`](Self::on_start)  - Called once before the event loop starts
/// - [`on_shutdown`](Self::on_shutdown)  - Called once after the event loop stops
/// - [`on_error`](Self::on_error)  - Handle errors (swallow or propagate)
///
/// # Context (Optional)
///
/// Actors that need to send events or stop themselves should store a [`Context<E>`](crate::Context)
/// received from their factory function. Pure event consumers don't need context:
///
/// ```ignore
/// // Pure consumer - no context needed
/// struct Logger;
/// impl Actor for Logger { /* ... */ }
///
/// // Producer - stores context to send events
/// struct Producer { ctx: Context<MyEvent> }
/// ```
///
/// # Ergonomics
///
/// Methods return futures but can be implemented as `async fn` directly.
/// No `#[async_trait]` macro is required.
///
/// See also: [`Context`](crate::Context), [`Supervisor`](crate::Supervisor), [`Envelope`](crate::Envelope).
pub trait Actor: Send + 'static {
    type Event: Event + Send;

    /// Handle a single incoming event.
    ///
    /// Receives the full [`Envelope`] containing both the event payload and metadata.
    /// Use `envelope.event()` for pattern matching, or access `envelope.meta()` for
    /// sender information and parent event IDs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// async fn handle_event(&mut self, envelope: &Envelope<Self::Event>) -> Result<()> {
    ///     match envelope.event() {
    ///         MyEvent::Foo(x) => self.handle_foo(x).await,
    ///         MyEvent::Bar => {
    ///             // Access metadata when needed
    ///             println!("Bar from {}", envelope.meta().actor_name());
    ///             Ok(())
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// Called for every event routed to this actor. Return `Ok(())` when
    /// processing succeeds, or an error to signal failure. Use `Context::send`
    /// to emit follow-up events as needed.
    fn handle_event(
        &mut self,
        envelope: &Envelope<Self::Event>,
    ) -> impl Future<Output = Result<()>> + Send {
        let _ = envelope;
        async { Ok(()) }
    }

    /// Optional periodic work or event production.
    ///
    /// Returns a [`StepAction`] to control when `step` runs again:
    ///
    /// | Action | Behavior |
    /// |--------|----------|
    /// | `StepAction::Continue` | Run step again immediately |
    /// | `StepAction::Yield` | Yield to runtime, then run again |
    /// | `StepAction::AwaitEvent` | Pause until next event arrives |
    /// | `StepAction::Backoff(Duration)` | Sleep, then run again |
    /// | `StepAction::Never` | Disable step permanently (default) |
    ///
    /// # Common Patterns
    ///
    /// **Time-Based Producer** (polls periodically):
    /// ```rust,ignore
    /// async fn step(&mut self) -> Result<StepAction> {
    ///     self.ctx.send(HeartbeatEvent).await?;
    ///     Ok(StepAction::Backoff(Duration::from_secs(5)))
    /// }
    /// ```
    ///
    /// **External Event Source** (driven by I/O):
    /// ```rust,ignore
    /// async fn step(&mut self) -> Result<StepAction> {
    ///     let frame = self.websocket.read().await?;
    ///     self.ctx.send(WebSocketEvent(frame)).await?;
    ///     Ok(StepAction::Continue)
    /// }
    /// ```
    ///
    /// **Pure Event Processor** (no step logic needed):
    /// ```rust,ignore
    /// async fn step(&mut self) -> Result<StepAction> {
    ///     Ok(StepAction::Never)  // Default behavior
    /// }
    /// ```
    ///
    /// # Default Behavior
    ///
    /// Returns `StepAction::Never`, making the actor purely event-driven.
    fn step(&mut self) -> impl Future<Output = Result<StepAction>> + Send {
        async { Ok(StepAction::default()) }
    }

    /// Lifecycle hook called once before the event loop starts.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn on_start(&mut self) -> Result<()>;
    /// ```
    fn on_start(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    /// Lifecycle hook called once after the event loop stops.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn on_shutdown(&mut self) -> Result<()>;
    /// ```
    fn on_shutdown(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    /// Called when an error is returned by [`handle_event`](Self::handle_event) or [`step`](Self::step).
    ///
    /// Return `Ok(())` to swallow the error and continue processing,
    /// or `Err(error)` to propagate and stop the actor.
    ///
    /// # Default Behavior
    ///
    /// By default, all errors propagate (actor stops). Override this to implement
    /// custom error handling, logging, or recovery logic.
    ///
    /// # Example
    ///
    /// ```rust
    /// use maiko::{Actor, Error, Event, Result};
    /// # #[derive(Clone, Event)]
    /// # struct MyEvent;
    /// # struct MyActor;
    /// # impl Actor for MyActor {
    /// #     type Event = MyEvent;
    /// fn on_error(&self, error: Error) -> Result<()> {
    ///     eprintln!("Actor error: {}", error);
    ///     Ok(())  // Swallow and continue
    /// }
    /// # }
    /// ```
    fn on_error(&self, error: Error) -> Result<()> {
        Err(error)
    }
}
