use naia_client::ReplicationConfig;
use naia_shared::WorldMutType;

use crate::{TestEntity, Position};
use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Context for client-side actions
pub struct ClientMutateCtx<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
}

impl<'a> ClientMutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, client_key: ClientKey) -> Self {
        Self {
            scenario,
            client_key,
        }
    }

    /// Begin spawning a new entity
    pub fn spawn(&mut self) -> ClientSpawnBuilder<'_> {
        ClientSpawnBuilder::new(self.scenario, self.client_key)
    }

    /// Get a mutable view of an entity
    pub fn entity(&mut self, entity: EntityKey) -> ClientEntityMut<'_> {
        ClientEntityMut::new(self.scenario, self.client_key, entity)
    }
}

/// Builder for spawning entities with components
pub struct ClientSpawnBuilder<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
    position: Option<Position>,
}

impl<'a> ClientSpawnBuilder<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, client_key: ClientKey) -> Self {
        Self {
            scenario,
            client_key,
            position: None,
        }
    }

    /// Attach a Position component to the entity
    pub fn with_position(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    /// Finalize the spawn and return the EntityKey
    pub fn track(self) -> EntityKey {
        let state = self.scenario.client_state_mut(self.client_key);
        
        // Spawn entity on client
        let mut entity_mut = state
            .client
            .spawn_entity(state.world.proxy_mut());

        if let Some(pos) = self.position {
            entity_mut.insert_component(pos);
        }

        // Configure entity as Public so it can be replicated to other clients
        // This must be done on the client side, not the server side
        entity_mut.configure_replication(ReplicationConfig::Public);

        let client_entity = entity_mut.id();
        
        // Get LocalEntity immediately from the client entity
        let client_ref = state.client.entity(state.world.proxy(), &client_entity);
        let local_entity = client_ref.local_entity()
            .expect("Client-spawned entity should have LocalEntity immediately");

        // Allocate EntityKey
        let entity_key = self.scenario.entity_registry_mut().allocate_entity_key();
        
        // Register the spawning client and their LocalEntity for direct lookup
        // This allows us to get the LocalEntity directly from the client later
        self.scenario.entity_registry_mut()
            .register_spawning_client(entity_key, self.client_key, local_entity);
        
        // Tick until server has entity, then register it
        // For client-spawned entities, the server receives a SpawnEntityEvent
        // The SpawnEntityEvent contains the server's entity ID directly
        let user_key = self.scenario.user_key(self.client_key);
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 200;
        
        loop {
            attempts += 1;
            if attempts > MAX_ATTEMPTS {
                panic!("ClientSpawnBuilder::track() timed out after {} ticks waiting for server to have entity with LocalEntity {:?}", MAX_ATTEMPTS, local_entity);
            }
            
            self.scenario.tick_once();
            
            // Check if server has SpawnEntityEvent for this user
            // The SpawnEntityEvent contains the server entity directly - use it!
            // Note: The LocalEntity on the server will be "Remote" while on the client it's "Host",
            // but they represent the same logical entity, so we can't compare them directly.
            let mut server_events = self.scenario.take_server_events();
            for (spawn_user_key, spawn_entity) in server_events.read::<naia_server::SpawnEntityEvent>() {
                if spawn_user_key == user_key {
                    // Found the spawn event - this is the server's entity for the client-spawned entity
                    // Verify it exists in the server world and register it
                    let server = self.scenario.server();
                    let server_world = self.scenario.server_world();
                    if server.entities(server_world.proxy()).contains(&spawn_entity) {
                        // Register the server entity - this is the host entity for this EntityKey
                        self.scenario.entity_registry_mut()
                            .register_host_entity(entity_key, spawn_entity);
                        return entity_key;
                    }
                }
            }
        }

        entity_key
    }
}

/// Mutable view of an entity on a client
pub struct ClientEntityMut<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
    entity_key: EntityKey,
}

impl<'a> ClientEntityMut<'a> {
    pub(crate) fn new(
        scenario: &'a mut Scenario,
        client_key: ClientKey,
        entity_key: EntityKey,
    ) -> Self {
        Self {
            scenario,
            client_key,
            entity_key,
        }
    }

    /// Get the client-side entity via LocalEntity
    fn get_entity(&mut self) -> TestEntity {
        let user_key = self.scenario.user_key(self.client_key);
        let local_entity = self.scenario.local_entity_for(self.entity_key, user_key)
            .expect("EntityKey not registered or not replicated to client");
        let state = self.scenario.client_state_mut(self.client_key);
        let world_proxy = state.world.proxy();
        let client_ref = state.client.local_entity(world_proxy, &local_entity)
            .expect("Entity should exist on client with this LocalEntity");
        client_ref.id()
    }

    /// Configure replication to use delegated/authority-based replication
    pub fn delegate(mut self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        let mut world_mut = state.world.proxy_mut();
        state
            .client
            .configure_entity_replication(&mut world_mut, &entity, ReplicationConfig::Delegated);
    }

    /// Request authority over this entity
    pub fn request_auth(mut self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        state.client.entity_request_authority(&entity);
    }

    /// Release authority over this entity
    pub fn release_auth(mut self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        state.client.entity_release_authority(&entity);
    }

    /// Set/update the position of the entity
    pub fn set_position(mut self, position: Position) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        
        // Use EntityMut which handles both insert and update
        let mut world_mut = state.world.proxy_mut();
        let mut entity_mut = state.client.entity_mut(world_mut, &entity);
        entity_mut.insert_component(position);
    }
}

