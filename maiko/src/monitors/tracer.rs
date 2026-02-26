use crate::{ActorId, Envelope, Event, OverflowPolicy, Topic, monitoring::Monitor};

/// A monitor that logs event lifecycle to the `tracing` crate.
///
/// Provides visibility into event flow without custom code. Log levels:
/// - `trace` - event dispatched/delivered/overflow (high volume)
/// - `debug` - event handled
/// - `warn` - errors
/// - `info` - actor stopped
///
/// # Example
///
/// ```ignore
/// use maiko::monitors::Tracer;
///
/// sup.monitors().add(Tracer).await;
/// ```
#[derive(Debug)]
pub struct Tracer;

impl<E, T> Monitor<E, T> for Tracer
where
    E: Event + std::fmt::Debug,
    T: Topic<E> + std::fmt::Debug,
{
    fn on_event_dispatched(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        tracing::trace!(
            event_id = %envelope.id(),
            sender = %envelope.meta().actor_name(),
            receiver = %receiver.as_str(),
            topic = ?topic,
            "event dispatched"
        );
    }

    fn on_event_delivered(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        tracing::trace!(
            event_id = %envelope.id(),
            receiver = %receiver.as_str(),
            topic = ?topic,
            "event delivered"
        );
    }

    fn on_event_handled(&self, envelope: &Envelope<E>, topic: &T, receiver: &ActorId) {
        tracing::debug!(
            event_id = %envelope.id(),
            sender = %envelope.meta().actor_name(),
            receiver = %receiver.as_str(),
            topic = ?topic,
            event = ?envelope.event(),
            "event handled"
        );
    }

    fn on_error(&self, err: &str, actor_id: &ActorId) {
        tracing::warn!(
            actor = %actor_id.as_str(),
            error = %err,
            "actor error"
        );
    }

    fn on_actor_stop(&self, actor_id: &ActorId) {
        tracing::info!(
            actor = %actor_id.as_str(),
            "actor stopped"
        );
    }

    fn on_actor_registered(&self, actor_id: &ActorId) {
        tracing::trace!(
            actor = %actor_id.as_str(),
            "actor registered"
        )
    }

    fn on_overflow(
        &self,
        envelope: &Envelope<E>,
        topic: &T,
        receiver: &ActorId,
        policy: OverflowPolicy,
    ) {
        tracing::trace!(
            event_id = %envelope.id(),
            receiver = %receiver.as_str(),
            topic = ?topic,
            policy = %policy,
            "overflow"
        );
    }
}
