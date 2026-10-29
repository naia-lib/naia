use std::time::Duration;

use futures_util::future::{select, Either};

/// Async wake-up signal with tokio-`Notify` semantics, built on a
/// 1-capacity channel instead of the tokio runtime.
///
/// `notify_one` stores a single permit: a waiter that arrives later still
/// wakes up, so the check-then-wait loops in `Buffer::read` and
/// `Stream::read_sctp` cannot miss a wakeup. `notify_waiters` closes the
/// channel, which wakes every current and future waiter; close is permanent,
/// matching the call sites (buffer/stream shutdown).
#[derive(Clone, Debug)]
pub(crate) struct Notify {
    tx: smol::channel::Sender<()>,
    rx: smol::channel::Receiver<()>,
}

impl Notify {
    pub(crate) fn new() -> Self {
        let (tx, rx) = smol::channel::bounded(1);
        Self { tx, rx }
    }

    pub(crate) fn notify_one(&self) {
        let _ = self.tx.try_send(());
    }

    pub(crate) fn notify_waiters(&self) {
        self.tx.close();
    }

    pub(crate) async fn notified(&self) {
        let _ = self.rx.recv().await;
    }

    /// Waits up to `duration` for a notification. Returns `true` when
    /// notified, `false` on timeout.
    pub(crate) async fn notified_timeout(&self, duration: Duration) -> bool {
        // `Box::pin` (not `.fuse()`): `select!` needs `Unpin` named futures,
        // and `Recv` borrows `self`, so the future cannot be a temporary.
        let notified = Box::pin(self.notified());
        let timer = Box::pin(smol::Timer::after(duration));
        matches!(select(notified, timer).await, Either::Left(_))
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}
