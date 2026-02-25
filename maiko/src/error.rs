use std::sync::Arc;

use tokio::sync::{
    broadcast,
    mpsc::error::{SendError, TrySendError},
};

use crate::{ActorId, Envelope};

/// The single error type for all Maiko operations.
///
/// Every fallible Maiko API returns `maiko::Result<T>` (alias for
/// `Result<T, maiko::Error>`). Errors from lower layers (Tokio channels,
/// IO, task joins) are mapped into variants of this enum so callers only
/// need to handle one error type.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Channel closed: {0}")]
    SendError(String),

    #[error("Actor task failed: {0}")]
    ActorJoinError(String),

    #[error("Broker has already started.")]
    BrokerAlreadyStarted,

    #[error("The message channel has reached its capacity.")]
    ChannelIsFull,

    #[error("Actor '{0}' already exists.")]
    DuplicateActorName(ActorId),

    #[error("External error: {0}")]
    External(#[source] Arc<dyn std::error::Error + Send + Sync>),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Not enough data to build an envelope")]
    EnvelopeBuildError,

    #[cfg(feature = "test-harness")]
    #[error("settle_on condition not met within {0:?}: {1} events recorded")]
    SettleTimeout(std::time::Duration, usize),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SendError(a), Self::SendError(b)) => a == b,
            (Self::ActorJoinError(a), Self::ActorJoinError(b)) => a == b,
            (Self::BrokerAlreadyStarted, Self::BrokerAlreadyStarted) => true,
            (Self::ChannelIsFull, Self::ChannelIsFull) => true,
            (Self::DuplicateActorName(a), Self::DuplicateActorName(b)) => a == b,
            (Self::External(a), Self::External(b)) => Arc::ptr_eq(a, b),
            (Self::IoError(a), Self::IoError(b)) => a == b,
            (Self::EnvelopeBuildError, Self::EnvelopeBuildError) => true,
            #[cfg(feature = "test-harness")]
            (Self::SettleTimeout(a1, a2), Self::SettleTimeout(b1, b2)) => a1 == b1 && a2 == b2,
            _ => false,
        }
    }
}

impl Eq for Error {}

impl<E> From<SendError<Arc<Envelope<E>>>> for Error {
    fn from(e: SendError<Arc<Envelope<E>>>) -> Self {
        Error::SendError(e.to_string())
    }
}

impl<E> From<TrySendError<Arc<Envelope<E>>>> for Error {
    fn from(e: TrySendError<Arc<Envelope<E>>>) -> Self {
        match e {
            TrySendError::Full(_) => Error::ChannelIsFull,
            TrySendError::Closed(_) => Error::SendError(e.to_string()),
        }
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Error::ActorJoinError(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e.to_string())
    }
}

impl<E> From<broadcast::error::SendError<E>> for Error {
    fn from(error: broadcast::error::SendError<E>) -> Self {
        Error::SendError(error.to_string())
    }
}
