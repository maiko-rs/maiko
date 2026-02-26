use std::sync::{Arc, atomic::AtomicBool};

use tokio::sync::oneshot;

use crate::{
    Config, Event, Topic,
    monitoring::{
        Monitor, MonitorCommand, MonitorDispatcher, MonitorHandle, MonitorId, MonitoringSink,
    },
};

/// Registry for managing monitors attached to a supervisor.
///
/// Access via [`Supervisor::monitors()`](crate::Supervisor::monitors).
///
/// # Example
///
/// ```ignore
/// let registry = supervisor.monitors();
///
/// // Add a monitor
/// let handle = registry.add(MyMonitor).await;
///
/// // Pause all monitors
/// registry.pause().await;
///
/// // Resume all monitors
/// registry.resume().await;
/// ```
pub struct MonitorRegistry<E: Event, T: Topic<E>> {
    dispatcher: Option<MonitorDispatcher<E, T>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
    pub(crate) sender: tokio::sync::mpsc::Sender<MonitorCommand<E, T>>,
    pub(crate) is_active: Arc<AtomicBool>,
}

impl<E: Event, T: Topic<E>> MonitorRegistry<E, T> {
    pub(crate) fn new(config: &Config) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(config.monitoring_channel_capacity());
        let is_active = Arc::new(AtomicBool::new(false));
        let dispatcher = MonitorDispatcher::new(rx, is_active.clone());
        Self {
            sender: tx,
            dispatcher: Some(dispatcher),
            dispatcher_handle: None,
            is_active,
        }
    }

    pub(crate) fn start(&mut self) {
        let mut dispatcher = self
            .dispatcher
            .take()
            .expect("Dispatcher must exist on start");
        let handle = tokio::spawn(async move {
            dispatcher.run().await;
        });
        self.dispatcher_handle = Some(handle);
    }

    pub(crate) async fn stop(&mut self) {
        match (
            self.sender.send(MonitorCommand::Shutdown).await,
            self.dispatcher_handle.take(),
        ) {
            (Ok(_), Some(handle)) => {
                let _ = handle.await;
            }
            (Err(_), Some(handle)) => {
                handle.abort();
            }
            _ => {}
        }
    }

    pub(crate) fn sink(&self) -> MonitoringSink<E, T> {
        MonitoringSink::new(self.sender.clone(), self.is_active.clone())
    }

    /// Register a new monitor and return a handle for controlling it.
    ///
    /// The monitor starts in the active (non-paused) state.
    ///
    /// # Panics
    ///
    /// Panics if the dispatcher channel is closed (supervisor already stopped).
    pub async fn add<M: Monitor<E, T> + 'static>(&self, monitor: M) -> MonitorHandle<E, T> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(MonitorCommand::AddMonitor(Box::new(monitor), tx))
            .await;
        let id = rx
            .await
            .expect("Monitor Id should be retrieved successfully");
        MonitorHandle::new(id, self.sender.clone())
    }

    /// Remove a monitor by its ID.
    ///
    /// Prefer using [`MonitorHandle::remove()`] instead.
    pub async fn remove(&self, id: MonitorId) {
        let _ = self.sender.send(MonitorCommand::RemoveMonitor(id)).await;
    }

    /// Pause all registered monitors.
    ///
    /// Paused monitors do not receive callbacks. Events continue to flow
    /// through the system normally.
    pub async fn pause(&self) {
        let _ = self.sender.send(MonitorCommand::PauseAll).await;
    }

    /// Resume all registered monitors.
    pub async fn resume(&self) {
        let _ = self.sender.send(MonitorCommand::ResumeAll).await;
    }
}

impl<E: Event, T: Topic<E>> Drop for MonitorRegistry<E, T> {
    fn drop(&mut self) {
        if !self.sender.is_closed() {
            let _ = self.sender.try_send(MonitorCommand::Shutdown);
        }
        if let Some(handle) = self.dispatcher_handle.take() {
            handle.abort();
        }
    }
}
