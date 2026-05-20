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
//! Per cyberlith `_AGENTS/GAME_CELL_PIPELINING_TARGET.md` §3.1, SubApp
//! ownership lives on the consumer's orchestrator (Coord). Naia
//! spawning an internal Send worker would conflict with cyberlith's
//! `pipelined_recv.rs`-style orchestration. Instead this commit ships
//! the consumer-facing API shape (the Sim Resource + `send`) and a
//! matching `take_latest()` for the Send-side consumer to drain.
//! Cyberlith's Send SubApp wiring becomes:
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

use std::{hash::Hash, sync::Arc};

use parking_lot::Mutex;

use naia_shared::SnapshotWorld;

/// Sim-side outbound snapshot sender. Cloneable (Arc-internal).
///
/// Install as a Resource on Sim's bevy world by
/// [`crate::pipeline_actors::SimHandle::snapshot_sender_pair`] (which
/// returns a `(SnapshotSender<E>, SnapshotReceiver<E>)` matched pair).
/// The matching [`SnapshotReceiver`] lives wherever the Send-side
/// `send_all_packets` runs (typically a Send SubApp).
pub struct SnapshotSender<E: Copy + Eq + Hash + Send + Sync + 'static> {
    slot: Arc<Mutex<Option<SnapshotWorld<E>>>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for SnapshotSender<E> {
    fn clone(&self) -> Self {
        Self {
            slot: Arc::clone(&self.slot),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> SnapshotSender<E> {
    /// Construct a matched sender/receiver pair.
    pub fn pair() -> (Self, SnapshotReceiver<E>) {
        let slot = Arc::new(Mutex::new(None));
        (
            Self {
                slot: Arc::clone(&slot),
            },
            SnapshotReceiver { slot },
        )
    }

    /// Publish a snapshot for the next `send_all_packets` to consume.
    ///
    /// Latest-wins: if the receiver hasn't drained yet, this overwrites
    /// the pending snapshot. The previous snapshot drops.
    pub fn send(&self, snapshot: SnapshotWorld<E>) {
        let mut guard = self.slot.lock();
        *guard = Some(snapshot);
    }

    /// Returns `true` if a snapshot has been published and not yet
    /// drained. Useful for telemetry; not load-bearing for correctness.
    pub fn has_pending(&self) -> bool {
        self.slot.lock().is_some()
    }
}

/// Send-side consumer half of a [`SnapshotSender`]. Cloneable
/// (Arc-internal).
pub struct SnapshotReceiver<E: Copy + Eq + Hash + Send + Sync + 'static> {
    slot: Arc<Mutex<Option<SnapshotWorld<E>>>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for SnapshotReceiver<E> {
    fn clone(&self) -> Self {
        Self {
            slot: Arc::clone(&self.slot),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> SnapshotReceiver<E> {
    /// Take the latest pending snapshot, leaving the slot empty.
    /// Returns `None` if no snapshot has been published since the last
    /// drain.
    pub fn take_latest(&self) -> Option<SnapshotWorld<E>> {
        self.slot.lock().take()
    }
}
