//! Bevy `Resource` wrapper around [`naia_server::pipeline_actors::SimConverter`]
//! for MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19).
//!
//! Sim systems install the [`SimConverter`] Resource on cyberlith's Sim
//! Bevy world so per-tick code that needs to set an `EntityProperty`
//! (e.g. `EntityAssignment` messages, `AssetRef` components) can call
//! the converter directly without reassembling a `WorldServer`.
//!
//! # Construction
//!
//! ```ignore
//! // After `spawn_server_handles`:
//! let sim_converter = SimConverter::from_coord(&coord);
//! sim_app.insert_resource(sim_converter);
//! ```
//!
//! # Wire identity
//!
//! The underlying impl on `ServerShared<E>` delegates to the same
//! `global_entity_map: RwLock<GlobalEntityMap<E>>` that
//! `WorldServer`'s `EntityAndGlobalEntityConverter<E>` impl reads, so
//! `EntityProperty::set(&*sim_converter, &entity)` produces a
//! byte-identical wire payload to the legacy
//! `run_with_naia_server(|ws| EntityProperty::set(ws, &entity))` path.

use bevy_ecs::{entity::Entity, resource::Resource};

use naia_server::pipeline_actors::{CoordHandle, SimConverter as InnerSimConverter};
use naia_bevy_shared::{EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity};

/// Bevy `Resource` wrapper over the cloneable Sim-side converter.
#[derive(Resource, Clone)]
pub struct SimConverter {
    inner: InnerSimConverter<Entity>,
}

impl SimConverter {
    /// Construct from a [`CoordHandle`] (which holds the shared
    /// `Arc<ServerShared<Entity>>` backing the converter).
    pub fn from_coord(coord: &CoordHandle<Entity>) -> Self {
        Self {
            inner: coord.sim_converter(),
        }
    }

    /// Borrow as a `&dyn EntityAndGlobalEntityConverter<Entity>`.
    pub fn as_dyn(&self) -> &dyn EntityAndGlobalEntityConverter<Entity> {
        self.inner.as_dyn()
    }
}

impl EntityAndGlobalEntityConverter<Entity> for SimConverter {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        self.inner.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.inner.entity_to_global_entity(world_entity)
    }
}
