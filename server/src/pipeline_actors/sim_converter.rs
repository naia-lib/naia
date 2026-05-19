//! `SimConverter<E>` — MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19).
//!
//! Sim-installable, cloneable `EntityAndGlobalEntityConverter<E>` view.
//! Cyberlith's Sim systems install this as a Bevy `Resource` (via the
//! bevy adapter's `Resource` derive on the re-export) and can then call
//! `EntityProperty::set(&*sim_converter, &entity)` without reassembling
//! a `WorldServer<E>`.
//!
//! # Why this exists
//!
//! Today the only `EntityAndGlobalEntityConverter<E>` implementer is
//! `WorldServer<E>` (`server/src/server/world_server.rs:3956`), whose
//! body just calls `self.shared.global_entity_map.read().*` — i.e. the
//! converter only needs `Arc<ServerShared<E>>`, not the fused server.
//! `SimConverter<E>` exposes exactly that subset as a cloneable handle
//! so cyberlith's `services/game/cell/src/server_access.rs` can retire
//! its per-tick `run_with_naia_server` reassembly (see Phase C of the
//! mission plan).
//!
//! # Wire identity
//!
//! Because the backing impl on `ServerShared<E>` (in
//! `server/src/server/server_shared.rs`) delegates to the same
//! `global_entity_map: RwLock<GlobalEntityMap<E>>` that
//! `WorldServer`'s converter reads, `EntityProperty::set` produces a
//! byte-identical `GlobalEntity` payload across the two paths. The
//! `coord_configure_entity_replication` test in this adapter exercises
//! this invariant end-to-end.
//!
//! # Construction
//!
//! Cyberlith builds a `SimConverter<E>` from any of the three pipeline
//! handles (they all hold the same `Arc<ServerShared<E>>`):
//!
//! ```ignore
//! let sim_converter = coord.sim_converter();
//! sim_app.insert_resource(sim_converter);
//! ```
//!
//! See [`crate::pipeline_actors::CoordHandle::sim_converter`].

use std::{hash::Hash, sync::Arc};

use naia_shared::{EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity};

/// Sim-installable, cloneable `EntityAndGlobalEntityConverter<E>`.
///
/// The inner `Arc` allows the converter to be moved across thread
/// boundaries and to be installed as a Bevy `Resource` (the bevy
/// adapter applies `#[derive(Resource)]` via the re-export at
/// `naia_bevy_server::SimConverter`).
pub struct SimConverter<E: Copy + Eq + Hash + Send + Sync + 'static> {
    inner: Arc<dyn EntityAndGlobalEntityConverter<E> + Send + Sync>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> SimConverter<E> {
    /// Wrap an arbitrary `Arc`-backed converter. The typical
    /// construction site is [`crate::pipeline_actors::CoordHandle::sim_converter`],
    /// which wraps the shared `Arc<ServerShared<E>>`.
    pub fn from_arc(inner: Arc<dyn EntityAndGlobalEntityConverter<E> + Send + Sync>) -> Self {
        Self { inner }
    }

    /// Borrow the inner trait object as a `&dyn EntityAndGlobalEntityConverter<E>`.
    /// Use this with `EntityProperty::set(sim_converter.as_dyn(), &entity)`.
    pub fn as_dyn(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        &*self.inner
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for SimConverter<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> EntityAndGlobalEntityConverter<E>
    for SimConverter<E>
{
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.inner.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.inner.entity_to_global_entity(world_entity)
    }
}
