use std::{fmt, sync::Arc};

use tokio::sync::mpsc::Sender;

use crate::{
    ActorId, Envelope, EnvelopeBuilder, EventId, Result,
    internal::{Command, CommandSender},
};

/// Runtime-provided context for an actor to interact with the system.
///
/// Use it to:
/// - `send(event)`: emit events into the broker tagged with this actor's ID
/// - `send_child_event(event, parent_id)`: emit an event linked to a parent event
/// - `stop()`: stop this actor (other actors continue running)
/// - `stop_runtime()`: initiate shutdown of the entire runtime
/// - `actor_id()`: retrieve the actor's identity
/// - `is_sender_full()`: check channel congestion before sending
///
/// See also: [`Envelope`], [`crate::Meta`], [`crate::Supervisor`].
#[derive(Clone)]
pub struct Context<E> {
    actor_id: ActorId,
    sender: Sender<Arc<Envelope<E>>>,
    cmd_tx: CommandSender,
}

impl<E> Context<E> {
    pub(crate) fn new(
        actor_id: ActorId,
        sender: Sender<Arc<Envelope<E>>>,
        cmd_tx: CommandSender,
    ) -> Self {
        Self {
            actor_id,
            sender,
            cmd_tx,
        }
    }

    /// Send an event to the broker. Accepts any type that converts into `E`.
    /// The envelope will carry this actor's name.
    /// This awaits channel capacity (backpressure) to avoid silent drops.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MailboxClosed`](crate::Error::MailboxClosed) if the broker
    /// channel is closed.
    pub async fn send<T: Into<EnvelopeBuilder<E>>>(&self, builder: T) -> Result {
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
    /// Returns [`Error::MailboxClosed`](crate::Error::MailboxClosed) if the broker
    /// channel is closed.
    pub async fn send_child_event<T: Into<EnvelopeBuilder<E>>>(
        &self,
        builder: T,
        parent_id: EventId,
    ) -> Result {
        let envelope = builder
            .into()
            .with_actor_id(self.actor_id.clone())
            .build()?
            .with_parent_id(parent_id);
        self.send_envelope(envelope).await
    }

    #[inline]
    async fn send_envelope<T: Into<Envelope<E>>>(&self, envelope: T) -> Result {
        self.sender.send(Arc::new(envelope.into())).await?;
        Ok(())
    }

    /// Stop this actor.
    ///
    /// The actor's event loop will exit after the current tick completes,
    /// and [`on_shutdown`](crate::Actor::on_shutdown) will be called.
    /// Other actors continue running.
    ///
    /// To shut down the entire runtime instead, use [`stop_runtime()`](Self::stop_runtime).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`](crate::Error::Internal) if the command
    /// channel is closed (runtime already shut down).
    #[inline]
    pub fn stop(&self) -> Result {
        self.cmd_tx.send(Command::StopActor(self.actor_id.clone()))
    }

    /// Initiate shutdown of the entire runtime.
    ///
    /// All actors will be cancelled and the supervisor's
    /// [`join()`](crate::Supervisor::join) or [`run()`](crate::Supervisor::run)
    /// call will return.
    ///
    /// To stop only this actor, use [`stop()`](Self::stop).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`](crate::Error::Internal) if the command
    /// channel is closed (runtime already shut down).
    #[inline]
    pub fn stop_runtime(&self) -> Result {
        self.cmd_tx.send(Command::StopRuntime)
    }

    /// The identity of this actor.
    #[inline]
    pub fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// The actor's name as registered with the supervisor.
    #[inline]
    pub fn actor_name(&self) -> &str {
        self.actor_id.as_str()
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
            .field("sender", &self.sender)
            .finish()
    }
}
