use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use crate::{ActorId, Envelope, Topic, internal::Subscription};

#[derive(Debug)]
pub(crate) struct Subscriber<T: Topic> {
    pub actor_id: ActorId,
    pub topics: Subscription<T>,
    pub sender: Sender<Arc<Envelope<T::Event>>>,
}

impl<T: Topic> Subscriber<T> {
    pub fn new(
        actor_id: ActorId,
        topics: Subscription<T>,
        sender: Sender<Arc<Envelope<T::Event>>>,
    ) -> Subscriber<T> {
        Subscriber {
            actor_id,
            topics,
            sender,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

impl<T: Topic> PartialEq for Subscriber<T> {
    fn eq(&self, other: &Self) -> bool {
        self.actor_id == other.actor_id
    }
}

impl<T: Topic> Eq for Subscriber<T> {}
