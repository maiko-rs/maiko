use std::sync::Arc;

use tokio::sync::mpsc::error::{SendError, TrySendError};

use crate::{ActorId, Envelope};

/// The single error type for all Maiko operations.
///
/// Every fallible Maiko API returns `maiko::Result<T>` (alias for
/// `Result<T, maiko::Error>`). Errors from lower layers (Tokio channels,
/// IO, task joins) are mapped into variants of this enum so callers only
/// need to handle one error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Actor's context must be set by this point")]
    ContextNotSet,

    #[error("Couldn't send the message: {0}")]
    SendError(String),

    #[error("Actor task join error: {0}")]
    ActorJoinError(#[from] tokio::task::JoinError),

    #[error("Broker has already started.")]
    BrokerAlreadyStarted,

    #[error("The message channel has reached its capacity.")]
    ChannelIsFull,

    #[error("Subscriber with name '{0}' already exists.")]
    SubscriberAlreadyExists(ActorId),

    #[error("Error external to Maiko occured: {0}")]
    External(Arc<str>),

    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),

    #[cfg(feature = "test-harness")]
    #[error("settle_on condition not met within {0:?}: {1} events recorded")]
    SettleTimeout(std::time::Duration, usize),
}

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
