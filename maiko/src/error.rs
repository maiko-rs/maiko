use std::sync::Arc;

use tokio::sync::mpsc::error::{SendError, TrySendError};

use crate::{ActorId, Envelope};

/// The single error type for all Maiko operations.
///
/// Every fallible Maiko API returns `maiko::Result<T>` (alias for
/// `Result<T, maiko::Error>`). Errors from lower layers (Tokio channels,
/// IO, task joins) are mapped into variants of this enum so callers only
/// need to handle one error type.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Mailbox closed")]
    MailboxClosed,

    #[error("Broker has already started.")]
    BrokerAlreadyStarted,

    #[error("The mailbox is full")]
    MailboxFull,

    #[error("Actor '{0}' already exists.")]
    DuplicateActorName(ActorId),

    #[error("External error: {0}")]
    External(#[source] Arc<dyn std::error::Error + Send + Sync>),

    #[error("IO error: {0}")]
    IoError(#[source] Arc<std::io::Error>),

    #[error("Internal Maiko error {0}")]
    Internal(#[source] Arc<dyn std::error::Error + Send + Sync>),

    #[cfg(feature = "test-harness")]
    #[error("settle_on condition not met within {0:?}: {1} events recorded")]
    SettleTimeout(std::time::Duration, usize),
}

impl Error {
    pub fn external(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::External(Arc::new(e))
    }

    pub(crate) fn internal(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::Internal(Arc::new(e))
    }
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MailboxClosed, Self::MailboxClosed) => true,
            (Self::MailboxFull, Self::MailboxFull) => true,
            (Self::BrokerAlreadyStarted, Self::BrokerAlreadyStarted) => true,
            (Self::DuplicateActorName(a), Self::DuplicateActorName(b)) => a == b,
            (Self::External(a), Self::External(b)) => Arc::ptr_eq(a, b),
            (Self::Internal(a), Self::Internal(b)) => Arc::ptr_eq(a, b),
            (Self::IoError(a), Self::IoError(b)) => Arc::ptr_eq(a, b),
            #[cfg(feature = "test-harness")]
            (Self::SettleTimeout(a1, a2), Self::SettleTimeout(b1, b2)) => a1 == b1 && a2 == b2,
            _ => false,
        }
    }
}

impl Eq for Error {}

impl<E> From<SendError<Arc<Envelope<E>>>> for Error {
    fn from(_e: SendError<Arc<Envelope<E>>>) -> Self {
        Error::MailboxClosed
    }
}

impl<E> From<TrySendError<Arc<Envelope<E>>>> for Error {
    fn from(e: TrySendError<Arc<Envelope<E>>>) -> Self {
        match e {
            TrySendError::Full(_) => Error::MailboxFull,
            TrySendError::Closed(_) => Error::MailboxClosed,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(Arc::new(e))
    }
}
