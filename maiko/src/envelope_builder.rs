use crate::{ActorId, Envelope, Event, EventId};

#[derive(Debug, Clone)]
pub struct EnvelopeBuilder<E> {
    envelope: Option<Envelope<E>>,
    event: Option<E>,
    actor_id: Option<ActorId>,
    parent_id: Option<EventId>,
}

impl<E> EnvelopeBuilder<E> {
    pub fn build(self) -> Envelope<E> {
        if let Some(envelope) = self.envelope {
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
            panic!("EnvelopeBuilder requires either an envelope or an event");
        }
    }

    pub fn with_actor_id(mut self, actor_id: ActorId) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    pub fn with_parent_id(mut self, parent_id: EventId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

impl<E> From<Envelope<E>> for EnvelopeBuilder<E> {
    fn from(value: Envelope<E>) -> Self {
        Self {
            envelope: Some(value),
            event: None,
            actor_id: None,
            parent_id: None,
        }
    }
}

impl<E: Event> From<E> for EnvelopeBuilder<E> {
    fn from(event: E) -> Self {
        Self {
            envelope: None,
            event: Some(event),
            actor_id: None,
            parent_id: None,
        }
    }
}
