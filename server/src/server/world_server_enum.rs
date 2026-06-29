//! The unified, framework-agnostic server handle (MISSION_PIPELINE_API_BOUNDARY
//! G-unify Phase 2c).
//!
//! [`WorldServer<E>`] is the ONE public face over the two engine shapes:
//!
//! - [`WorldServerImpl::Resident`] — the fused [`InternalWorldServer`] driven
//!   synchronously on the calling thread.
//! - [`WorldServerImpl::Pipelined`] — the *same* engine's three handles split
//!   across worker threads ([`PipelinedWorldServer`]), assembled into a
//!   transient [`InternalWorldServer`] view per drive.
//!
//! Both variants are built from the same three handles (`CoordHandle` /
//! `RecvHandle` / `SendHandle`), so a consumer holds ONE type, picks its drive
//! shape at construction ([`WorldServer::new`] vs [`WorldServer::new_pipelined`]),
//! and gets the same op surface + `receive`/`send` drives dispatched per variant.
//! This is what makes pipelining ergonomic for non-bevy naia consumers without
//! reassembling the engine by hand.
//!
//! The op/drive surface is grown across the G-unify phases: Phase 2c stands up
//! the shell (construction + lifecycle); Phase 3 adds the unified `receive`/`send`
//! drives; Phase 4 adds the `entity_mut` builder.

use std::hash::Hash;

use naia_shared::{Protocol, Tick, WorldMutType, WorldRefType};

use crate::{
    world::entity_mut::{EntityMut, EntityMutTarget},
    InternalWorldServer, PipelinedWorldServer, ReceiveOutput, ServerConfig,
};

/// Whether a [`WorldServer`] drives its engine synchronously (resident) or
/// across the pipeline's worker threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    /// Synchronous, single-threaded drive over the fused engine.
    Resident,
    /// Worker-thread pipelined drive (park-window brackets).
    Pipelined,
}

/// The unified server handle. See the module docs.
pub struct WorldServer<E: Copy + Eq + Hash + Send + Sync + 'static> {
    inner: WorldServerImpl<E>,
}

/// Internal variant carrier. Kept private so the only way to act on a
/// [`WorldServer`] is through its dispatched surface (no variant-pattern leak).
enum WorldServerImpl<E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(InternalWorldServer<E>),
    Pipelined(PipelinedWorldServer<E>),
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> WorldServer<E> {
    /// Construct a **resident** server: the fused engine, driven synchronously.
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        Self {
            inner: WorldServerImpl::Resident(InternalWorldServer::new(server_config, protocol)),
        }
    }

    /// Construct a **pipelined** server: the engine's handles split across the
    /// worker-thread park-window runtime.
    pub fn new_pipelined<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        Self {
            inner: WorldServerImpl::Pipelined(PipelinedWorldServer::new(server_config, protocol)),
        }
    }

    /// Which drive shape this server was constructed with.
    pub fn mode(&self) -> ServerMode {
        match &self.inner {
            WorldServerImpl::Resident(_) => ServerMode::Resident,
            WorldServerImpl::Pipelined(_) => ServerMode::Pipelined,
        }
    }

    /// Bind the transport socket and begin listening for clients. For the
    /// pipelined variant this reassembles the engine to run `io_load` exactly
    /// as the resident variant does (the worker threads are not yet spawned).
    pub fn listen<S: Into<Box<dyn crate::transport::Socket>>>(&mut self, socket: S) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                let (_auth_tx, _auth_rx, ps, pr) =
                    crate::transport::Socket::listen(socket.into());
                ws.io_load(ps, pr);
            }
            WorldServerImpl::Pipelined(ps) => ps.listen(socket),
        }
    }

    /// The current server tick.
    pub fn current_tick(&self) -> Tick {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.current_tick(),
            WorldServerImpl::Pipelined(ps) => ps.current_tick(),
        }
    }

    /// Receive one tick's worth of inbound traffic and apply it to `world`,
    /// returning the per-source [`ReceiveOutput`]s (world + tick events).
    ///
    /// The world is taken **by value** — the established naia convention is a
    /// fresh per-call proxy (`world.proxy_mut()`). The resident variant drives
    /// its fused `receive_with_world` (one output); the pipelined variant drains
    /// its recv path (one output in the oracle shape, N in the worker shape).
    /// Both ultimately apply through the same `process_all_packets` path.
    pub fn receive<W: WorldMutType<E>>(&mut self, mut world: W) -> Vec<ReceiveOutput<E>> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => vec![ws.receive_with_world(world)],
            WorldServerImpl::Pipelined(ps) => ps.receive(&mut world),
        }
    }

    /// Flush one tick's worth of outbound traffic, serialized against `world`.
    ///
    /// World by value (fresh per-call proxy, e.g. `world.proxy()`). The resident
    /// variant transmits inline against the live world; the pipelined variant
    /// transmits the frozen needed-set snapshot (oracle inline, or published to
    /// the send worker). Byte-identical modulo the worker's one-tick lag
    /// (g9pre).
    pub fn send<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_all_packets(world),
            WorldServerImpl::Pipelined(ps) => ps.send(&world),
        }
    }

    /// The imperative entity builder, dispatched per variant. Holds the `world`
    /// for component access. `entity` must already exist in `world`.
    ///
    /// ```ignore
    /// server.entity_mut(world, &entity)
    ///       .enable_replication()
    ///       .configure_replication(ReplicationConfig::public())
    ///       .enter_room(&room_key);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `entity` does not exist in `world` (matching the resident
    /// `InternalWorldServer::entity_mut` contract).
    pub fn entity_mut<W: WorldMutType<E>>(&mut self, world: W, entity: &E) -> EntityMut<'_, E, W> {
        if !world.has_entity(entity) {
            panic!("No Entity exists for given Key!");
        }
        let target = match &mut self.inner {
            WorldServerImpl::Resident(ws) => EntityMutTarget::Resident(ws),
            WorldServerImpl::Pipelined(ps) => EntityMutTarget::Pipelined(ps),
        };
        EntityMut::with_target(target, world, entity)
    }
}
