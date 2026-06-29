//! [`SimPipeline<E>`] — unified pipeline handle returned by
//! [`spawn_server_handles`](super::spawn_server_handles).
//!
//! Owns all three sub-handles:
//! - `sim` (coordination, main-thread only).
//! - `recv` (network-receive worker) — in a park-window slot shared with the worker.
//! - `send` (network-send worker) — in a park-window slot shared with the worker.
//!
//! # Primary API: `tick`
//!
//! ```ignore
//! sim_pipeline.tick(&mut world, |ctx| {
//!     ctx.host_sync();          // bevy-adapter extension
//!     ctx.send_all_packets();   // bevy-adapter extension
//! });
//! ```
//!
//! The body receives a [`TickCtx`] with owned `recv`/`send` handles and a
//! `&mut SimHandle<E>` for the duration of the tick. Workers must be parked
//! before calling `tick()`; [`SimPipeline`] does NOT own the park/unpark
//! machinery — that belongs to the adapter (`PluginInternalState::park_workers`).
//!
//! # Worker slot sharing
//!
//! [`SimPipeline::recv_slot`] and [`SimPipeline::send_slot`] return
//! `Arc<Mutex<Option<...>>>` clones. The bevy adapter gives these to the
//! recv/send workers at spawn time; workers deposit their handle before
//! parking and re-claim it after unpark.

use std::{hash::Hash, sync::Arc};

use parking_lot::Mutex;
use naia_shared::WorldMutType;

use crate::{RecvHandle, SendHandle};

use super::handles::SimHandle;

// ── SimPipeline ──────────────────────────────────────────────────────────────

/// Unified pipeline handle returned by [`super::spawn_server_handles`].
///
/// See module-level docs for the design overview.
pub struct SimPipeline<E: Copy + Eq + Hash + Send + Sync> {
    /// Coordination handle (main-thread only). Stored as `Option` so the
    /// handle can be temporarily moved into a `WorldServer` for operations
    /// that require full-server access (e.g., `io_load` in G2).
    pub(super) sim: Option<SimHandle<E>>,
    /// Park-window slot shared with the recv worker.
    recv_slot: Arc<Mutex<Option<RecvHandle<E>>>>,
    /// Park-window slot shared with the send worker.
    send_slot: Arc<Mutex<Option<SendHandle<E>>>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> SimPipeline<E> {
    pub(super) fn new(sim: SimHandle<E>, recv: RecvHandle<E>, send: SendHandle<E>) -> Self {
        Self {
            sim: Some(sim),
            recv_slot: Arc::new(Mutex::new(Some(recv))),
            send_slot: Arc::new(Mutex::new(Some(send))),
        }
    }

    /// Borrow the coordination handle.
    ///
    /// Panics if `sim` is temporarily absent (only during [`Self::tick`] or
    /// [`Self::with_world_server`] — both restore it before returning).
    pub fn sim(&self) -> &SimHandle<E> {
        self.sim.as_ref().expect("SimPipeline: SimHandle temporarily unavailable")
    }

    /// Mutably borrow the coordination handle.
    pub fn sim_mut(&mut self) -> &mut SimHandle<E> {
        self.sim.as_mut().expect("SimPipeline: SimHandle temporarily unavailable")
    }

    /// `Arc` clone of the recv worker's park-window slot.
    ///
    /// The bevy adapter passes this to the recv worker at spawn time so the
    /// worker can deposit/re-claim its [`RecvHandle`] across park windows.
    pub fn recv_slot(&self) -> Arc<Mutex<Option<RecvHandle<E>>>> {
        Arc::clone(&self.recv_slot)
    }

    /// `Arc` clone of the send worker's park-window slot.
    ///
    /// Symmetric to [`Self::recv_slot`].
    pub fn send_slot(&self) -> Arc<Mutex<Option<SendHandle<E>>>> {
        Arc::clone(&self.send_slot)
    }

    /// Run one pipelined tick.
    ///
    /// Moves `recv` and `send` out of their slots, passes a [`TickCtx`] to
    /// `body`, then returns both handles to their slots. `sim` is also
    /// temporarily moved to allow framework-agnostic body implementations
    /// that need owned access.
    ///
    /// # Precondition
    ///
    /// Workers must be parked before calling `tick()` — this ensures the
    /// handles are in their slots. The bevy adapter wraps this with
    /// `PluginInternalState::park_workers` / `unpark_workers`.
    pub fn tick<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        body: impl FnOnce(&mut TickCtx<'_, E, W>),
    ) {
        let mut sim = self.sim.take().expect(
            "SimPipeline::tick: SimHandle not available — re-entrant tick?",
        );
        let recv = self.recv_slot.lock().take().expect(
            "SimPipeline::tick: RecvHandle not in slot — park workers before calling tick()",
        );
        let send = self.send_slot.lock().take().expect(
            "SimPipeline::tick: SendHandle not in slot — park workers before calling tick()",
        );
        let mut ctx = TickCtx {
            sim: &mut sim,
            recv,
            send,
            world,
        };
        body(&mut ctx);
        // Destructure to release the `sim` borrow (ctx.sim = &mut sim) before
        // moving sim back into the Option slot. The reference fields (sim, world)
        // are bound to `_` and dropped here, expiring their borrows.
        let TickCtx { sim: _, recv: recv_out, send: send_out, world: _ } = ctx;
        self.sim = Some(sim);
        *self.recv_slot.lock() = Some(recv_out);
        *self.send_slot.lock() = Some(send_out);
    }

    /// Take only the coordination handle, leaving recv/send slots untouched.
    ///
    /// Used by the bevy adapter's `drain_recv_impl_split` which already has
    /// independent access to the recv/send Arcs (via `RecvHandleRes` /
    /// `SendHandleRes`). MUST be followed by [`Self::restore_sim`] before the
    /// pipeline is used again.
    pub fn take_sim(&mut self) -> SimHandle<E> {
        self.sim.take().expect("SimPipeline::take_sim: SimHandle not available")
    }

    /// Restore a coordination handle previously taken by [`Self::take_sim`].
    pub fn restore_sim(&mut self, sim: SimHandle<E>) {
        self.sim = Some(sim);
    }

    /// Take all three handles for external manipulation.
    ///
    /// Workers must be parked before calling this. Returns `(sim, recv, send)`.
    /// MUST be followed by [`Self::restore_handles`] before workers are unparked
    /// or [`Self::tick`] is called. Used by the bevy adapter's `drain_recv_impl`
    /// which passes the handles through `apply_recv_to_world` (a loop that requires
    /// owned handle access and cannot use the `tick()` closure form).
    pub fn take_handles(&mut self) -> (SimHandle<E>, RecvHandle<E>, SendHandle<E>) {
        let sim = self.sim.take().expect(
            "SimPipeline::take_handles: SimHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "SimPipeline::take_handles: RecvHandle not in slot — park workers first",
        );
        let send = self.send_slot.lock().take().expect(
            "SimPipeline::take_handles: SendHandle not in slot — park workers first",
        );
        (sim, recv, send)
    }

    /// Restore handles previously taken by [`Self::take_handles`].
    pub fn restore_handles(&mut self, sim: SimHandle<E>, recv: RecvHandle<E>, send: SendHandle<E>) {
        self.sim = Some(sim);
        *self.recv_slot.lock() = Some(recv);
        *self.send_slot.lock() = Some(send);
    }

    /// Temporarily reassemble a [`crate::WorldServer`] and invoke `f` against it.
    ///
    /// All three handles are moved into the `WorldServer`; after `f` returns
    /// they are re-split and restored. Used internally and in tests that need
    /// full-server access (e.g. `io_load` until G2 lands, `entity_replication_config`
    /// until G3 adds it to `SimHandle`).
    ///
    /// Workers must be parked (or not yet started) before calling this.
    pub fn with_world_server<R>(
        &mut self,
        f: impl FnOnce(&mut crate::WorldServer<E>) -> R,
    ) -> R
    where
        E: 'static,
    {
        use std::sync::Arc;
        use crate::server::WorldServer;

        let sim_handle = self.sim.take().expect(
            "SimPipeline::with_world_server: SimHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "SimPipeline::with_world_server: RecvHandle not in slot",
        );
        let send = self.send_slot.lock().take().expect(
            "SimPipeline::with_world_server: SendHandle not in slot",
        );

        let coord_state = sim_handle.state;
        let mut ws = WorldServer::from_pipeline_states(coord_state, recv.state, send.state);
        let result = f(&mut ws);
        let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
        let shared = Arc::clone(&recv_state.shared);

        self.sim = Some(SimHandle { state: coord_state, shared });
        *self.recv_slot.lock() = Some(RecvHandle { state: recv_state });
        *self.send_slot.lock() = Some(SendHandle { state: send_state });

        result
    }
}

// ── TickCtx ──────────────────────────────────────────────────────────────────

/// Scoped context passed to a [`SimPipeline::tick`] body.
///
/// Provides framework-agnostic access to all three pipeline handles plus the
/// consumer's world. The recv and send handles are **owned** for the duration
/// of the tick body; [`SimPipeline::tick`] automatically returns them to their
/// slots when the body returns.
///
/// Adapters (e.g., `naia-bevy-server`) may add extension methods by `impl`ing
/// `TickCtx<'_, Entity, SpecificWorldType>` in their own crate.
pub struct TickCtx<'a, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    /// Coordination handle (borrowed from the pipeline for this tick).
    pub sim: &'a mut SimHandle<E>,
    /// Receive handle (owned by this context for this tick).
    pub recv: RecvHandle<E>,
    /// Send handle (owned by this context for this tick).
    pub send: SendHandle<E>,
    /// Consumer's world (framework-specific).
    pub world: &'a mut W,
}
