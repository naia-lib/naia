use async_lock::Mutex;
use async_trait::async_trait;
use futures_util::{select, FutureExt};
use smol::channel;
use std::sync::Weak;
use std::time::Duration;

pub(crate) const ACK_INTERVAL: Duration = Duration::from_millis(200);

/// ackTimerObserver is the inteface to an ack timer observer.
#[async_trait]
pub(crate) trait AckTimerObserver {
    async fn on_ack_timeout(&mut self);
}

/// ackTimer provides the retnransmission timer conforms with RFC 4960 Sec 6.3.1
#[derive(Default, Debug)]
pub(crate) struct AckTimer<T: 'static + AckTimerObserver + Send> {
    pub(crate) timeout_observer: Weak<Mutex<T>>,
    pub(crate) interval: Duration,
    pub(crate) close_tx: Option<channel::Sender<()>>,
}

impl<T: 'static + AckTimerObserver + Send> AckTimer<T> {
    /// newAckTimer creates a new acknowledgement timer used to enable delayed ack.
    pub(crate) fn new(timeout_observer: Weak<Mutex<T>>, interval: Duration) -> Self {
        AckTimer {
            timeout_observer,
            interval,
            close_tx: None,
        }
    }

    /// start starts the timer.
    pub(crate) fn start(&mut self) -> bool {
        // this timer is already closed
        if self.close_tx.is_some() {
            return false;
        }

        let (close_tx, close_rx) = channel::bounded(1);
        let interval = self.interval;
        let timeout_observer = self.timeout_observer.clone();

        smol::spawn(async move {
            select! {
                _ = smol::Timer::after(interval).fuse() => {
                    if let Some(observer) = timeout_observer.upgrade(){
                        let mut observer = observer.lock().await;
                        observer.on_ack_timeout().await;
                    }
                 }
                _ = close_rx.recv().fuse() => {},
            }
        })
        .detach();

        self.close_tx = Some(close_tx);
        true
    }

    /// stops the timer. this is similar to stop() but subsequent start() call
    /// will fail (the timer is no longer usable)
    pub(crate) fn stop(&mut self) {
        self.close_tx.take();
    }
}
