use std::{hash::Hash, sync::Arc};

use parking_lot::Mutex;
use naia_shared::WorldRefType;

use super::{world_server::WorldServer, receive_output::ReceiveOutput};

/// Pipeline-facing receive handle.
///
/// Wraps `WorldServer<E>` behind an `Arc<Mutex<>>` so it can be sent to a
/// dedicated recv thread.  In Phase 3 both `RecvHandle` and `SendHandle`
/// point to the **same** `WorldServer` instance; the mutex serialises access.
/// True concurrent execution (recv and send running simultaneously) requires
/// the field-level split planned for Phase 4.
///
/// # Thread safety
///
/// `WorldServer<E>` may contain raw pointers internally, so we assert `Send`
/// manually.  This is safe because the mutex guarantees exclusive access.
pub struct RecvHandle<E: Copy + Eq + Hash + Send + Sync> {
    pub(super) world_server: Arc<Mutex<WorldServer<E>>>,
}

// SAFETY: access is always gated by the parking_lot::Mutex.
unsafe impl<E: Copy + Eq + Hash + Send + Sync> Send for RecvHandle<E> {}

impl<E: Copy + Eq + Hash + Send + Sync> RecvHandle<E> {
    /// Run the full receive phase and return decoded events.
    ///
    /// Calls [`WorldServer::receive_all_packets`] then drains accumulated
    /// world events into a [`ReceiveOutput`].
    ///
    /// Note: [`WorldServer::process_all_packets`] is NOT called here because
    /// it requires a `World` reference — the caller is responsible for
    /// invoking that separately (or the pipeline coordinator handles it).
    ///
    /// Called from the recv thread in the pipeline coordinator.
    pub fn receive(&mut self) -> ReceiveOutput<E> {
        let mut ws = self.world_server.lock();
        ws.receive()
    }
}

/// Pipeline-facing send handle.
///
/// Mirrors [`RecvHandle`] — same `Arc<Mutex<WorldServer<E>>>` under the hood.
/// Phase 3: serialised via mutex.  Phase 4: will hold a separate `WorldSend`
/// half after the field split.
pub struct SendHandle<E: Copy + Eq + Hash + Send + Sync> {
    pub(super) world_server: Arc<Mutex<WorldServer<E>>>,
}

// SAFETY: access is always gated by the parking_lot::Mutex.
unsafe impl<E: Copy + Eq + Hash + Send + Sync> Send for SendHandle<E> {}

impl<E: Copy + Eq + Hash + Send + Sync> SendHandle<E> {
    /// Flush all outbound packets to connected clients.
    ///
    /// Forwards to [`WorldServer::send_all_packets`].
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        let mut ws = self.world_server.lock();
        ws.send_all_packets(world);
    }
}
