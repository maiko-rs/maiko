use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::mpsc::Sender;

use crate::{ActorId, Envelope, EnvelopeBuilder, EventId, Result};

/// Runtime-provided context for an actor to interact with the system.
///
/// Use it to:
/// - `send(event)`: emit events into the broker tagged with this actor's ID
/// - `send_child_event(event, parent_id)`: emit an event linked to a parent event
/// - `stop()`: request graceful shutdown of this actor
/// - `actor_id()`: retrieve the actor's identity
/// - `is_alive()`: check whether the actor loop should continue running
///
/// See also: [`Envelope`], [`crate::Meta`], [`crate::Supervisor`].
#[derive(Clone)]
pub struct Context<E> {
    actor_id: ActorId,
    sender: Sender<Arc<Envelope<E>>>,
    alive: Arc<AtomicBool>,
}

impl<E> Context<E> {
    pub(crate) fn new(
        actor_id: ActorId,
        sender: Sender<Arc<Envelope<E>>>,
        alive: Arc<AtomicBool>,
    ) -> Self {
        Self {
            actor_id,
            sender,
            alive,
        }
    }

    /// Send an event to the broker. Accepts any type that converts into `E`.
    /// The envelope will carry this actor's name.
    /// This awaits channel capacity (backpressure) to avoid silent drops.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SendError`](crate::Error::SendError) if the broker
    /// channel is closed.
    pub async fn send<T: Into<EnvelopeBuilder<E>>>(&self, builder: T) -> Result<()> {
        let envelope = builder
            .into()
            .with_actor_id(self.actor_id.clone())
            .build()?;
        self.send_envelope(envelope).await
    }

    /// Emit a child event linked to the given parent event ID.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SendError`](crate::Error::SendError) if the broker
    /// channel is closed.
    pub async fn send_child_event<T: Into<EnvelopeBuilder<E>>>(
        &self,
        builder: T,
        parent_id: EventId,
    ) -> Result<()> {
        let envelope = builder
            .into()
            .with_actor_id(self.actor_id.clone())
            .build()?
            .with_parent_id(parent_id);
        self.send_envelope(envelope).await
    }

    #[inline]
    async fn send_envelope<T: Into<Envelope<E>>>(&self, envelope: T) -> Result<()> {
        self.sender.send(Arc::new(envelope.into())).await?;
        Ok(())
    }

    /// Signal this actor to stop
    #[inline]
    pub fn stop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }

    #[inline]
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// The actor's name as registered with the supervisor.
    #[inline]
    pub fn actor_name(&self) -> &str {
        self.actor_id.as_str()
    }

    /// Whether the actor is considered alive by the runtime.
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Returns a future that never completes.
    ///
    /// **Note:** This method is largely obsolete. The default `step()` implementation
    /// now returns `StepAction::Never`, which disables stepping entirely. You only need
    /// `pending()` if you want to block inside a custom `step()` implementation.
    ///
    /// ```rust,ignore
    /// // Prefer this (default behavior):
    /// async fn step(&mut self) -> Result<StepAction> {
    ///     Ok(StepAction::Never)
    /// }
    ///
    /// // Or simply don't implement step() at all - it defaults to Never
    /// ```
    #[inline]
    pub async fn pending(&self) -> Result<()> {
        std::future::pending::<()>().await;
        Ok(())
    }

    /// Whether this actor's channel to the broker has no remaining capacity.
    ///
    /// Producers can use this to skip sending non-essential events when
    /// the channel is congested (stage 1). This is a cooperative
    /// mechanism - the producer decides what to skip.
    ///
    /// ```rust,ignore
    /// // Skip telemetry when the channel is busy
    /// if !ctx.is_sender_full() {
    ///     ctx.send(Event::Telemetry(stats)).await?;
    /// }
    /// ```
    ///
    /// Note: each actor has its own channel to the broker (stage 1).
    /// This reflects the sending actor's individual backlog, not
    /// global system pressure or subscriber-side congestion (stage 2).
    #[inline]
    pub fn is_sender_full(&self) -> bool {
        self.sender.capacity() == 0
    }
}

impl<E> fmt::Debug for Context<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("actor_id", &self.actor_id)
            .field("is_alive", &self.is_alive())
            .field("sender", &self.sender)
            .finish()
    }
}
