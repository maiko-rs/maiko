use std::sync::Arc;

use crate::{ActorId, Envelope, OverflowPolicy, Topic};

pub(crate) enum MonitoringEvent<T: Topic> {
    EventDispatched(Arc<Envelope<T::Event>>, Arc<T>, ActorId),
    EventDelivered(Arc<Envelope<T::Event>>, Arc<T>, ActorId),
    EventHandled(Arc<Envelope<T::Event>>, Arc<T>, ActorId),
    Overflow(Arc<Envelope<T::Event>>, Arc<T>, ActorId, OverflowPolicy),
    ActorRegistered(ActorId),
    ActorStopped(ActorId),
    Error(Arc<str>, ActorId),
}
