use std::{
    collections::HashMap,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::{
    select,
    sync::{mpsc::Receiver, oneshot},
    time::Instant,
};

use crate::{
    Event, Topic,
    monitoring::{Monitor, MonitorCommand, MonitorId, MonitoringEvent},
};

struct MonitorEntry<E: Event, T: Topic<E>> {
    monitor: Box<dyn Monitor<E, T>>,
    paused: bool,
}

impl<E: Event, T: Topic<E>> MonitorEntry<E, T> {
    fn new(monitor: Box<dyn Monitor<E, T>>) -> Self {
        Self {
            monitor,
            paused: false,
        }
    }
}

pub(crate) struct MonitorDispatcher<E: Event, T: Topic<E>> {
    receiver: Receiver<MonitorCommand<E, T>>,
    monitors: HashMap<MonitorId, MonitorEntry<E, T>>,
    last_id: MonitorId,
    ids_to_remove: Vec<MonitorId>,
    is_active: Arc<AtomicBool>,
    flush_pending: Option<(oneshot::Sender<()>, Duration)>,
    last_activity: Instant,
    is_alive: bool,
}

impl<E: Event, T: Topic<E>> MonitorDispatcher<E, T> {
    pub fn new(receiver: Receiver<MonitorCommand<E, T>>, is_active: Arc<AtomicBool>) -> Self {
        Self {
            receiver,
            monitors: HashMap::new(),
            last_id: 0,
            ids_to_remove: Vec::with_capacity(8),
            is_active,
            flush_pending: None,
            last_activity: Instant::now(),
            is_alive: true,
        }
    }

    fn update_is_active(&mut self) {
        let active = self.monitors.values().any(|m| !m.paused);
        self.is_active.store(active, Ordering::Relaxed);
    }

    fn remove_monitor(&mut self, id: MonitorId) {
        self.monitors.remove(&id);
        self.update_is_active();
    }

    fn set_monitor_paused(&mut self, id: MonitorId, paused: bool) {
        if let Some(entry) = self.monitors.get_mut(&id) {
            entry.paused = paused;
            self.update_is_active();
        }
    }

    fn set_monitors_paused_to_all(&mut self, paused: bool) {
        for entry in self.monitors.values_mut() {
            entry.paused = paused;
        }
        self.update_is_active();
    }

    fn try_complete_flush(&mut self) {
        if let Some((_, settle_window)) = &self.flush_pending {
            if self.receiver.is_empty() && self.last_activity.elapsed() >= *settle_window {
                if let Some((response, _)) = self.flush_pending.take() {
                    let _ = response.send(());
                }
            }
        }
    }

    pub async fn run(&mut self) {
        const FLUSH_CHECK_INTERVAL: Duration = Duration::from_micros(100);

        while self.is_alive {
            select! {
                Some(cmd) = self.receiver.recv() => {
                    self.last_activity = Instant::now();
                    self.handle_command(cmd);
                }
                _ = tokio::time::sleep(FLUSH_CHECK_INTERVAL), if self.flush_pending.is_some() => {
                    self.try_complete_flush();
                }
            }
        }
    }

    fn handle_command(&mut self, cmd: MonitorCommand<E, T>) {
        use MonitorCommand::*;
        match cmd {
            AddMonitor(monitor, resp) => {
                let id = self.last_id;
                self.monitors.insert(id, MonitorEntry::new(monitor));
                self.last_id += 1;
                self.update_is_active();
                let _ = resp.send(id);
            }
            RemoveMonitor(id) => {
                self.remove_monitor(id);
            }
            PauseAll => {
                self.set_monitors_paused_to_all(true);
            }
            ResumeAll => {
                self.set_monitors_paused_to_all(false);
            }
            PauseOne(id) => {
                self.set_monitor_paused(id, true);
            }
            ResumeOne(id) => {
                self.set_monitor_paused(id, false);
            }
            DispatchEvent(event) if self.is_active.load(Ordering::Relaxed) => {
                self.handle_event(event);
            }
            Flush {
                response,
                settle_window,
            } => {
                self.flush_pending = Some((response, settle_window));
                self.try_complete_flush();
            }
            Shutdown => {
                self.is_alive = false;
            }
            _ => {}
        }
    }

    fn handle_event(&mut self, event: MonitoringEvent<E, T>) {
        use MonitoringEvent::*;
        match event {
            EventDispatched(envelope, topic, actor_id) => {
                self.notify(|m| m.on_event_dispatched(&envelope, &topic, &actor_id));
            }
            EventDelivered(envelope, topic, actor_id) => {
                self.notify(|m| m.on_event_delivered(&envelope, &topic, &actor_id));
            }
            EventHandled(envelope, topic, actor_id) => {
                self.notify(|m| m.on_event_handled(&envelope, &topic, &actor_id));
            }
            Overflow(envelope, topic, actor_id, policy) => {
                self.notify(|m| m.on_overflow(&envelope, &topic, &actor_id, policy));
            }
            Error(error, actor_id) => {
                self.notify(|m| m.on_error(&error, &actor_id));
            }
            ActorRegistered(actor_id) => {
                self.notify(|m| m.on_actor_registered(&actor_id));
            }
            ActorStopped(actor_id) => {
                self.notify(|m| m.on_actor_stop(&actor_id));
            }
        }
    }

    fn notify(&mut self, f: impl Fn(&dyn Monitor<E, T>)) {
        for (id, entry) in &self.monitors {
            if entry.paused {
                continue;
            }

            let result = catch_unwind(AssertUnwindSafe(|| f(entry.monitor.as_ref())));
            if result.is_err() {
                tracing::error!(monitor_id = %id, "Monitor panicked, removing");
                self.ids_to_remove.push(*id);
            }
        }

        while let Some(id) = self.ids_to_remove.pop() {
            self.remove_monitor(id);
        }
    }
}

impl<E: Event, T: Topic<E>> fmt::Debug for MonitorDispatcher<E, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MonitorDispatcher")
            .field("receiver", &self.receiver)
            .field("monitors.len()", &self.monitors.len())
            .field("last_id", &self.last_id)
            .field("ids_to_remove", &self.ids_to_remove)
            .field("is_active", &self.is_active)
            .field("flush_pending", &self.flush_pending)
            .field("last_activity", &self.last_activity)
            .field("is_alive", &self.is_alive)
            .finish()
    }
}
