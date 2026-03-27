use std::sync::Arc;

use crate::{ActorId, Envelope, OverflowPolicy, StepAction};

pub(crate) enum MonitoringEvent<E, T> {
    EventDispatched(Arc<Envelope<E>>, Arc<T>, ActorId),
    EventDelivered(Arc<Envelope<E>>, Arc<T>, ActorId),
    EventHandled(Arc<Envelope<E>>, Arc<T>, ActorId),
    StepEnter(ActorId),
    StepExit(StepAction, ActorId),
    Overflow(Arc<Envelope<E>>, Arc<T>, ActorId, OverflowPolicy),
    ActorRegistered(ActorId),
    ActorStopped(ActorId),
    Error(Arc<str>, ActorId),
}
