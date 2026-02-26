use crate::{ActorId, Envelope, Event, EventId};

/// Intermediate type used to pass events to [`Context::send`](crate::Context::send)
/// and [`Supervisor::send`](crate::Supervisor::send).
///
/// You don't interact with this type directly — any event type `E`
/// converts into it automatically via `From<E>`:
///
/// ```rust,ignore
/// ctx.send(MyEvent::Ping).await?;
/// ```
///
/// Created automatically via `From<E>` (for events) or
/// `From<Envelope<E>>` (for pre-built envelopes).
#[derive(Debug, Clone)]
pub struct IntoEnvelope<E> {
    envelope: Option<Envelope<E>>,
    event: Option<E>,
    actor_id: Option<ActorId>,
    parent_id: Option<EventId>,
}

impl<E> IntoEnvelope<E> {
    /// Consume the builder and produce an [`Envelope`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::EnvelopeBuildError`] if neither a pre-built envelope
    /// nor an event + actor ID were provided.
    pub(crate) fn build(self) -> Envelope<E> {
        let mut envelope = if let Some(envelope) = self.envelope {
            envelope
        } else if let Some(event) = self.event
            && let Some(actor_id) = self.actor_id
        {
            let e = Envelope::new(event, actor_id);
            if let Some(parent_id) = self.parent_id {
                e.with_parent_id(parent_id)
            } else {
                e
            }
        } else {
            panic!("build called without actor_id set");
        };

        if let Some(parent_id) = self.parent_id {
            envelope = envelope.with_parent_id(parent_id);
        }

        envelope
    }

    /// Set the sender's actor ID. Called internally by [`Context`](crate::Context).
    pub(crate) fn with_actor_id(mut self, actor_id: ActorId) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set the parent event ID for causality tracking.
    pub(crate) fn with_parent_id(mut self, parent_id: EventId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

impl<E> From<Envelope<E>> for IntoEnvelope<E> {
    fn from(value: Envelope<E>) -> Self {
        Self {
            envelope: Some(value),
            event: None,
            actor_id: None,
            parent_id: None,
        }
    }
}

impl<E: Event> From<E> for IntoEnvelope<E> {
    fn from(event: E) -> Self {
        Self {
            envelope: None,
            event: Some(event),
            actor_id: None,
            parent_id: None,
        }
    }
}
