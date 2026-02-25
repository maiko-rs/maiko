use crate::ActorId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    StopActor(ActorId),
    StopRuntime,
    StopBroker,
}
