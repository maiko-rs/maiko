use std::time::Duration;

use tokio::sync::oneshot;

use crate::{
    Topic,
    monitoring::{Monitor, MonitorId, MonitoringEvent},
};

pub(crate) enum MonitorCommand<T: Topic> {
    AddMonitor(Box<dyn Monitor<T>>, oneshot::Sender<MonitorId>),
    RemoveMonitor(MonitorId),
    PauseAll,
    ResumeAll,
    PauseOne(MonitorId),
    ResumeOne(MonitorId),
    DispatchEvent(MonitoringEvent<T>),
    /// Flush waits for the command queue to be empty and stay empty for the
    /// specified settle window before responding.
    Flush {
        response: oneshot::Sender<()>,
        settle_window: Duration,
    },
    Shutdown,
}
