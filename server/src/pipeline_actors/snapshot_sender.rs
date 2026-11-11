//! `SnapshotSender<E>` — C.6-prep Sim-side outbound snapshot facade.
//!
//! # Why this exists
//!
//! Today cyberlith ferries a `SnapshotWorld<E>` from Sim → Send via two
//! chained extracts and three `Pending*Input*` Resources. Per the
//! Medium-symmetric C.6 spec (§1.5), the desired Sim-facing API is a
//! single channel-backed `SnapshotSender<E>` Resource with one method:
//!
//! ```ignore
//! snapshot_tx.send(snapshot);
//! ```
//!
//! Latest-wins semantics: subsequent sends within the same tick replace
//! the previous snapshot (only the last one ships).
//!
//! # Why this is a facade, not an embedded Send worker
//!
//! Per cyberlith `_AGENTS/GAME_CELL_PIPELINING_TARGET.md` §3.1, `SubApp`
//! ownership lives on the consumer's orchestrator (Coord). Naia
//! spawning an internal Send worker would conflict with cyberlith's
//! `pipelined_recv.rs`-style orchestration. Instead this commit ships
//! the consumer-facing API shape (the Sim Resource + `send`) and a
//! matching `take_latest()` for the Send-side consumer to drain.
//! Cyberlith's Send `SubApp` wiring becomes:
//!
//! ```ignore
//! // On Sim:
//! snapshot_tx.send(build_snapshot(...));
//!
//! // On Send (extract or first system):
//! if let Some(snap) = snapshot_rx.take_latest() {
//!     send.apply_pending_send_preamble();
//!     send.send_all_packets(&snap);
//! }
//! ```
//!
//! The Send-side consumer half is constructed by
//! [`SnapshotSender::pair`] — both halves are cheap clones of an
//! internal `Arc<Mutex<Option<SnapshotWorld<E>>>>`.

use std::{
    hash::Hash,
    sync::{Arc, OnceLock},
};

use parking_lot::Mutex;

use naia_shared::SnapshotWorld;

use super::runtime::WorkerWake;

/// What both halves share: the latest-wins slot and, once the pipeline
/// runtime is armed, the send worker's idle wake (nydus-y2e) so a publish
/// wakes the worker instead of a 100µs poll finding the job.
struct Shared<E: Copy + Eq + Hash + Send + Sync + 'static> {
    slot: Mutex<Option<SnapshotWorld<E>>>,
    wake: OnceLock<Arc<WorkerWake>>,
}

/// Sim-side outbound snapshot sender. Cloneable (Arc-internal).
///
/// Install as a Resource on Sim's bevy world by
/// [`SnapshotSender::pair`] (which
/// returns a `(SnapshotSender<E>, SnapshotReceiver<E>)` matched pair).
/// The matching [`SnapshotReceiver`] lives wherever the Send-side
/// `send_all_packets` runs (typically a Send `SubApp`).
pub struct SnapshotSender<E: Copy + Eq + Hash + Send + Sync + 'static> {
    shared: Arc<Shared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for SnapshotSender<E> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> SnapshotSender<E> {
    /// Construct a matched sender/receiver pair.
    #[must_use]
    pub fn pair() -> (Self, SnapshotReceiver<E>) {
        let shared = Arc::new(Shared {
            slot: Mutex::new(None),
            wake: OnceLock::new(),
        });
        (
            Self {
                shared: Arc::clone(&shared),
            },
            SnapshotReceiver { shared },
        )
    }

    /// Publish a snapshot for the next `send_all_packets` to consume.
    ///
    /// Latest-wins: if the receiver hasn't drained yet, this overwrites
    /// the pending snapshot. The previous snapshot drops.
    pub fn send(&self, snapshot: SnapshotWorld<E>) {
        *self.shared.slot.lock() = Some(snapshot);
        // Wake the idle send worker (nydus-y2e). The slot write above happens
        // before `notify` takes the wake mutex, so a worker that found the slot
        // empty under that mutex is inside `wait()` and is woken.
        if let Some(wake) = self.shared.wake.get() {
            wake.notify();
        }
    }

    /// Returns `true` if a snapshot has been published and not yet
    /// drained. Useful for telemetry; not load-bearing for correctness.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.shared.slot.lock().is_some()
    }
}

/// Send-side consumer half of a [`SnapshotSender`]. Cloneable
/// (Arc-internal).
pub struct SnapshotReceiver<E: Copy + Eq + Hash + Send + Sync + 'static> {
    shared: Arc<Shared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for SnapshotReceiver<E> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> SnapshotReceiver<E> {
    /// Take the latest pending snapshot, leaving the slot empty.
    /// Returns `None` if no snapshot has been published since the last
    /// drain.
    #[must_use]
    pub fn take_latest(&self) -> Option<SnapshotWorld<E>> {
        self.shared.slot.lock().take()
    }

    /// Share the send worker's idle wake with the publisher, so
    /// [`SnapshotSender::send`] wakes the worker. Set once by
    /// [`crate::pipeline_actors::PipelineRuntime::new_armed`]; a later call is a
    /// no-op.
    pub(crate) fn attach_wake(&self, wake: Arc<WorkerWake>) {
        let _ = self.shared.wake.set(wake);
    }

    /// `true` if a snapshot has been published and not yet drained. Used by
    /// the send worker's flush-park drain to decide when the publish queue is
    /// empty (not load-bearing for the normal lagged path).
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.shared.slot.lock().is_some()
    }
}
