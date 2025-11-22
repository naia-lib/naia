//! BEVY ECS INTEGRATION MODULE
//!
//! This module provides basic integration types for naia with Bevy's ECS.
//!
//! **Note:** This module only depends on `bevy_ecs` (not full Bevy). For complete
//! Bevy integration, use the `naia-bevy-client` and `naia-bevy-server` adapters.
//!
//! # Usage
//!
//! ```rust,ignore
//! use naia_shared::bevy_integration::*;
//! use bevy_ecs::prelude::*;
//!
//! // In your Bevy app setup:
//! app.insert_resource(NaiaEntityMapping::default());
//!
//! // Spawn a replicated entity:
//! commands.spawn((
//!     Replicated,
//!     GlobalEntityId(global_entity),
//! ));
//! ```

#[cfg(feature = "bevy_ecs")]
mod bevy_integration_impl {
    use bevy_ecs::{component::Component, entity::Entity as BevyEntity, prelude::Resource};
    use std::collections::HashMap;

    use crate::GlobalEntity;

    /// Marker component for entities that should be replicated over the network
    #[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)] // Public API for external Bevy adapters
    pub struct Replicated;

    /// Marker component for entities controlled by the server
    #[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)] // Public API for external Bevy adapters
    pub struct ServerAuthority;

    /// Marker component for entities controlled by the client
    #[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)] // Public API for external Bevy adapters
    pub struct ClientAuthority;

    /// Component that tracks the global entity ID for network replication
    #[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)] // Public API for external Bevy adapters
    pub struct GlobalEntityId(pub GlobalEntity);

    /// Resource that manages the mapping between Bevy entities and global network entities
    ///
    /// This resource maintains a bidirectional mapping to allow efficient lookups
    /// in both directions during entity replication and migration.
    #[derive(Resource, Default, Debug)]
    #[allow(dead_code)] // Public API for external Bevy adapters
    pub struct NaiaEntityMapping {
        bevy_to_global: HashMap<BevyEntity, GlobalEntity>,
        global_to_bevy: HashMap<GlobalEntity, BevyEntity>,
    }

    #[allow(dead_code)] // Public API for external Bevy adapters
    impl NaiaEntityMapping {
        /// Create a new empty entity mapping
        pub fn new() -> Self {
            Self::default()
        }

        /// Register a mapping between a Bevy entity and a GlobalEntity
        pub fn register(&mut self, bevy_entity: BevyEntity, global_entity: GlobalEntity) {
            self.bevy_to_global.insert(bevy_entity, global_entity);
            self.global_to_bevy.insert(global_entity, bevy_entity);
        }

        /// Remove a mapping for a Bevy entity, returning the GlobalEntity if it existed
        pub fn unregister_bevy(&mut self, bevy_entity: BevyEntity) -> Option<GlobalEntity> {
            if let Some(global_entity) = self.bevy_to_global.remove(&bevy_entity) {
                self.global_to_bevy.remove(&global_entity);
                Some(global_entity)
            } else {
                None
            }
        }

        /// Remove a mapping for a GlobalEntity, returning the Bevy entity if it existed
        pub fn unregister_global(&mut self, global_entity: GlobalEntity) -> Option<BevyEntity> {
            if let Some(bevy_entity) = self.global_to_bevy.remove(&global_entity) {
                self.bevy_to_global.remove(&bevy_entity);
                Some(bevy_entity)
            } else {
                None
            }
        }

        /// Get the GlobalEntity for a Bevy entity
        pub fn get_global(&self, bevy_entity: BevyEntity) -> Option<GlobalEntity> {
            self.bevy_to_global.get(&bevy_entity).copied()
        }

        /// Get the Bevy entity for a GlobalEntity
        pub fn get_bevy(&self, global_entity: GlobalEntity) -> Option<BevyEntity> {
            self.global_to_bevy.get(&global_entity).copied()
        }

        /// Check if a Bevy entity is registered
        pub fn contains_bevy(&self, bevy_entity: BevyEntity) -> bool {
            self.bevy_to_global.contains_key(&bevy_entity)
        }

        /// Check if a GlobalEntity is registered
        pub fn contains_global(&self, global_entity: GlobalEntity) -> bool {
            self.global_to_bevy.contains_key(&global_entity)
        }

        /// Get the number of registered entities
        pub fn len(&self) -> usize {
            self.bevy_to_global.len()
        }

        /// Check if the mapping is empty
        pub fn is_empty(&self) -> bool {
            self.bevy_to_global.is_empty()
        }

        /// Clear all mappings
        pub fn clear(&mut self) {
            self.bevy_to_global.clear();
            self.global_to_bevy.clear();
        }
    }
}

// Re-export the public API when bevy_ecs feature is enabled
#[cfg(feature = "bevy_ecs")]
#[allow(unused_imports)] // These are re-exported for external use
pub use bevy_integration_impl::{
    ClientAuthority, GlobalEntityId, NaiaEntityMapping, Replicated, ServerAuthority,
};
