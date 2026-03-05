use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::mpsc::Sender;

use crate::{
    Topic,
    monitoring::{MonitorCommand, MonitoringEvent},
};

pub(crate) struct MonitoringSink<T: Topic> {
    is_active: Arc<AtomicBool>,
    sender: Sender<MonitorCommand<T>>,
}

impl<T: Topic> MonitoringSink<T> {
    pub fn new(sender: Sender<MonitorCommand<T>>, is_active: Arc<AtomicBool>) -> Self {
        Self { sender, is_active }
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn send(&self, event: MonitoringEvent<T>) {
        let msg = MonitorCommand::DispatchEvent(event);
        let _ = self.sender.try_send(msg);
    }
}
