//! Drop-propagated shutdown for one listening Socket's background tasks.
//!
//! `Socket::listen*` spawns three detached tasks (receiver, sender, session
//! listener) that park in futures which never observe the user handles being
//! dropped, so dropping the handles leaked the bound ports
//! (naia-lib/naia#92). Each task therefore also waits on its own one-shot:
//! all senders live in one shared [`ShutdownSignal`], cloned into every
//! returned handle, so the last handle drop resolves every wait, each task
//! exits, and both sockets are released.

use std::sync::Arc;

use futures_channel::oneshot;

/// Shared shutdown trigger for one listen: held (via [`Arc`]) by every
/// returned handle. The senders are never sent on; their only job is to die
/// with the last handle, which resolves every paired wait.
pub struct ShutdownSignal {
    #[allow(dead_code)]
    senders: Vec<oneshot::Sender<()>>,
}

/// One background task's shutdown wait. Resolves when the last handle drops
/// (sender-drop resolves the receiver just like a send would).
pub struct ShutdownWait {
    receiver: oneshot::Receiver<()>,
}

impl ShutdownWait {
    /// Resolves on shutdown. The outcome carries no information.
    pub async fn wait(self) {
        let _ = self.receiver.await;
    }
}

/// Builds one shared signal plus one wait per background task.
pub fn shutdown_set(task_count: usize) -> (Arc<ShutdownSignal>, Vec<ShutdownWait>) {
    let mut senders = Vec::with_capacity(task_count);
    let mut waits = Vec::with_capacity(task_count);
    for _ in 0..task_count {
        let (sender, receiver) = oneshot::channel();
        senders.push(sender);
        waits.push(ShutdownWait { receiver });
    }
    (Arc::new(ShutdownSignal { senders }), waits)
}
