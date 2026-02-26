use crate::ActorId;

/// Internal command sent over the broadcast channel to coordinate
/// shutdown between the supervisor, broker, and actor controllers.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Stop a single actor by ID.
    StopActor(ActorId),
    /// Shut down all actors.
    StopRuntime,
    /// Shut down the broker event loop.
    StopBroker,
}
