use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::Sender;

use crate::{Envelope, Error, Result};

/// Send into a Stage-1 channel with a shutdown gate and no post-check await gap.
///
/// Stage naming follows crate-level [Flow Control](crate#flow-control).
///
/// This solves a shutdown race:
/// 1. sender checks `stop_gate == false`,
/// 2. sender awaits capacity in `send(...).await`,
/// 3. shutdown flips `stop_gate` while sender is suspended,
/// 4. sender still enqueues after shutdown began.
///
/// [`Ordering::Acquire`] loads are used for `stop_gate` so they synchronize with the
/// [`Ordering::Release`] store performed by shutdown ([`crate::Supervisor::stop`]). When a sender
/// observes `true`, it also observes writes that happened-before shutdown
/// published that stop signal. `Relaxed` would keep atomicity of the flag, but
/// would not provide that visibility/synchronization guarantee.
///
/// The helper uses `reserve().await` to get capacity, re-checks `stop_gate`,
/// then calls `Permit::send` (non-async). That removes the check/await window
/// after the second gate check.
pub(crate) async fn gated_send<E>(
    stop_gate: &Arc<AtomicBool>,
    sender: &Sender<Arc<Envelope<E>>>,
    envelope: Arc<Envelope<E>>,
) -> Result {
    if stop_gate.load(Ordering::Acquire) {
        return Err(Error::MailboxClosed);
    }

    let permit = sender.reserve().await.map_err(|_| Error::MailboxClosed)?;

    if stop_gate.load(Ordering::Acquire) {
        drop(permit);
        return Err(Error::MailboxClosed);
    }

    permit.send(envelope);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::sync::mpsc::{self, error::TryRecvError};

    use crate::{ActorId, Envelope, Error, Event};

    use super::gated_send;

    #[derive(Clone, Debug)]
    struct TestEvent(u8);
    impl Event for TestEvent {}

    fn test_envelope(value: u8) -> Arc<Envelope<TestEvent>> {
        Arc::new(Envelope::new(TestEvent(value), ActorId::new("test-actor")))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gated_send_rejects_when_gate_is_already_set() {
        let stop_gate = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::channel(1);

        let res = gated_send(&stop_gate, &tx, test_envelope(1)).await;
        assert_eq!(res, Err(Error::MailboxClosed));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gated_send_rechecks_gate_after_waiting_for_capacity() {
        let stop_gate = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::channel(1);

        gated_send(&stop_gate, &tx, test_envelope(1))
            .await
            .expect("first send must fill the only slot");

        let stop_gate_task = stop_gate.clone();
        let tx_task = tx.clone();
        let task =
            tokio::spawn(
                async move { gated_send(&stop_gate_task, &tx_task, test_envelope(2)).await },
            );

        // Let spawned task park on `reserve()` while channel is full.
        tokio::task::yield_now().await;

        // Flip stop while sender is blocked on capacity.
        stop_gate.store(true, Ordering::Release);

        // Free one slot; without the second check this would enqueue event #2.
        let drained = rx.recv().await.expect("expected first queued event");
        assert_eq!(drained.event().0, 1);

        let task_res = task.await.expect("gated send task should not panic");
        assert_eq!(task_res, Err(Error::MailboxClosed));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
