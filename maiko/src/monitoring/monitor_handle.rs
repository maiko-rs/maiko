use std::time::Duration;

use tokio::sync::{mpsc::Sender, oneshot};

use crate::{
    Event, Topic,
    monitoring::{MonitorCommand, MonitorId},
};

/// Handle for controlling a registered monitor.
///
/// Returned by [`MonitorRegistry::add`](crate::monitoring::MonitorRegistry::add).
///
/// # Example
///
/// ```ignore
/// let handle = supervisor.monitors().add(MyMonitor).await;
///
/// // Pause this specific monitor
/// handle.pause().await;
///
/// // Resume it
/// handle.resume().await;
///
/// // Remove it entirely
/// handle.remove().await;
/// ```
#[derive(Debug, Clone)]
pub struct MonitorHandle<E: Event, T: Topic<E>> {
    id: MonitorId,
    sender: Sender<MonitorCommand<E, T>>,
}

impl<E: Event, T: Topic<E>> MonitorHandle<E, T> {
    pub(crate) fn new(id: MonitorId, sender: Sender<MonitorCommand<E, T>>) -> Self {
        Self { id, sender }
    }

    /// Returns this monitor's unique identifier.
    pub fn id(&self) -> MonitorId {
        self.id
    }

    /// Remove this monitor from the registry.
    ///
    /// Consumes the handle since the monitor can no longer be controlled.
    pub async fn remove(self) {
        let _ = self
            .sender
            .send(MonitorCommand::RemoveMonitor(self.id))
            .await;
    }

    /// Pause this monitor.
    ///
    /// Paused monitors do not receive callbacks. Events continue to flow
    /// through the system normally.
    pub async fn pause(&self) {
        let _ = self.sender.send(MonitorCommand::PauseOne(self.id)).await;
    }

    /// Resume this monitor.
    pub async fn resume(&self) {
        let _ = self.sender.send(MonitorCommand::ResumeOne(self.id)).await;
    }

    /// Flush waits for the monitoring dispatcher queue to be empty and stay
    /// empty for the specified settle window before returning.
    ///
    /// This ensures all events queued before the flush call have been processed.
    pub async fn flush(&self, settle_window: Duration) {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(MonitorCommand::Flush {
                response: tx,
                settle_window,
            })
            .await;
        let _ = rx.await;
    }
}
