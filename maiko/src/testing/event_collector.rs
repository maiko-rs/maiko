use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;

use crate::{ActorId, Envelope, Topic, monitoring::Monitor, testing::EventEntry};

#[derive(Debug)]
pub struct EventCollector<T: Topic> {
    events: UnboundedSender<EventEntry<T>>,
}

impl<T: Topic> EventCollector<T> {
    pub fn new(events: UnboundedSender<EventEntry<T>>) -> Self {
        Self { events }
    }
}

impl<T: Topic> Monitor<T> for EventCollector<T> {
    fn on_event_handled(&self, envelope: &Envelope<T::Event>, topic: &T, receiver: &ActorId) {
        let event = Arc::new(envelope.clone());
        let topic = Arc::new(topic.clone());
        let actor_id = receiver.clone();
        let entry = EventEntry::new(event, topic, actor_id);
        let _ = self.events.send(entry);
    }
}
