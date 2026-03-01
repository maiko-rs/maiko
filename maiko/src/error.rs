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
    /// The actor's mailbox channel was closed (actor or broker shut down).
    #[error("Mailbox closed")]
    MailboxClosed,

    /// Attempted to start the broker when it is already running.
    #[error("Broker has already started.")]
    BrokerAlreadyStarted,

    /// The actor's mailbox is full and the overflow policy rejected the event.
    #[error("The mailbox is full")]
    MailboxFull,

    /// An actor with this name is already registered.
    #[error("Actor '{0}' already exists.")]
    DuplicateActorName(ActorId),

    /// An application-level error wrapped via [`Error::external()`].
    #[error("External error: {0}")]
    External(#[source] Arc<dyn std::error::Error + Send + Sync>),

    /// An I/O error from the underlying system.
    #[error("IO error: {0}")]
    IoError(#[source] Arc<std::io::Error>),

    /// An internal Maiko error (e.g., task join failure).
    #[error("Internal Maiko error {0}")]
    Internal(#[source] Arc<dyn std::error::Error + Send + Sync>),

    /// A [`settle_on`](crate::testing::Harness::settle_on) condition was not
    /// met within the timeout. Contains the timeout duration and the number
    /// of events recorded before expiry.
    #[cfg(feature = "test-harness")]
    #[error("settle_on condition not met within {0:?}: {1} events recorded")]
    SettleTimeout(std::time::Duration, usize),
}

impl Error {
    /// Wrap an application-level error into [`Error::External`].
    ///
    /// Use this in [`Actor::handle_event`](crate::Actor::handle_event) or
    /// [`Actor::on_error`](crate::Actor::on_error) to propagate domain errors
    /// through the Maiko error type.
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
