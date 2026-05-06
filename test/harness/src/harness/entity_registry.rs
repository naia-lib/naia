use std::collections::HashMap;

use naia_shared::LocalEntity;

use crate::{
    harness::{ClientKey, EntityKey},
    TestEntity,
};

/// Record tracking all entity mappings for a logical EntityKey
///
/// Lifecycle:
/// - Initially created empty by `allocate_entity_key()` (pending state)
/// - Must have at least one entity registered before being used:
///   - Either `server_entity` via `register_server_entity()`
///   - Or at least one `client_entity` via `register_client_entity()`
/// - This invariant is enforced by mutate phase semantics, not runtime checks
struct EntityKeyRecord {
    /// Server host world entity (None for client-spawned entities until they replicate to server)
    server_entity: Option<TestEntity>,
    /// Client world entities - primarily stores the host client's TestEntity
    /// Remote clients are primarily identified via reverse LocalEntity mappings in `client_entity_to_entity_key`
    /// The LocalEntity is the same for Server<->Client pairs (same user), but different between clients
    client_entities: HashMap<ClientKey, TestEntity>,
    /// LocalEntity mappings for clients
    client_local_entities: HashMap<ClientKey, LocalEntity>,
}

impl EntityKeyRecord {
    fn new() -> Self {
        Self {
            server_entity: None,
            client_entities: HashMap::new(),
            client_local_entities: HashMap::new(),
        }
    }
}

/// EntityRegistry is the source of truth for all EntityKey mappings.
///
/// Maps logical EntityKeys (test harness abstraction) to their concrete
/// server and client TestEntity instances in the naia world.
///
/// # Entity Storage Patterns
///
/// - **Host client entities**: The host client's `TestEntity` is reliably stored in
///   `client_entities` map for direct lookup.
///
/// - **Remote client entities**: Remote clients are primarily identified via reverse
///   `(ClientKey, LocalEntity) -> EntityKey` mappings in `client_entity_to_entity_key`.
///   Remote clients may not have entries in `client_entities`; they rely on LocalEntity
///   mappings for entity resolution.
///
/// - **LocalEntity as join key**: LocalEntity equality (for a given user) is the primary
///   join key across server and clients. The same logical entity has the same LocalEntity
///   value for Server<->Client pairs (same user), but different LocalEntity values
///   between different clients viewing the same entity.
///
/// # Lifecycle and Invariants
///
/// ## Entity Creation
///
/// 1. **Allocation**: `allocate_entity_key()` creates a new EntityKey with an empty record.
///    This is a "pending" state - the record has no entities yet.
///
/// 2. **Registration**: Entities are registered in one of two ways:
///    - **Server-spawned**: `register_server_entity()` is called in mutate phase,
///      then `register_client_local_entity_mapping()` when server replicates to clients,
///      then `register_client_entity()` when client receives SpawnEntityEvent.
///    - **Client-spawned**: `register_client_entity()` is called in mutate phase,
///      which creates a pending entry, then `register_server_entity()` when server
///      receives SpawnEntityEvent (which removes the pending entry).
///
/// ## Pending Client Spawns
///
/// - **Creation**: When `register_client_entity()` is called and no server entity exists,
///   an entry is added to `pending_client_spawns`. This happens in the mutate phase.
///
/// - **Resolution**: When the server receives the corresponding `ServerSpawnEntityEvent`,
///   `Scenario::tick_once()` calls `remove_pending_client_spawn()` to consume the entry
///   and register the server entity. This is the ONLY place pending entries are removed.
///
/// - **Idempotency**: Calling `register_client_entity()` multiple times for the same
///   EntityKey is safe and will not create duplicate pending entries.
///
/// ## Invariants
///
/// - **Mature records**: After the first tick completes, each EntityKey should have
///   at least one entity registered (either server_entity or at least one client_entity).
///
/// - **Pending spawn uniqueness**: At most one pending client-spawned entity per ClientKey
///   at a time. If the same client tries to register multiple pending spawns, the second
///   registration will verify it's for the same EntityKey.
///
/// ## Assumptions
///
/// - Entity despawn is not currently supported - no cleanup methods exist.
///   This is acceptable for small test scenarios but may cause memory growth in large tests.
pub struct EntityRegistry {
    next_entity_key: u32,
    entity_map: HashMap<EntityKey, EntityKeyRecord>,
    /// Reverse mapping: (ClientKey, LocalEntity) -> EntityKey
    /// Primary lookup mechanism for remote clients when they receive SpawnEntityEvent.
    /// LocalEntity is the same for Server<->Client pairs (same user), different between clients.
    /// This is the primary way remote clients are identified; they may not have entries in `client_entities`.
    client_entity_to_entity_key: HashMap<(ClientKey, LocalEntity), EntityKey>,
    /// Reverse mapping: server TestEntity -> EntityKey
    /// Used to quickly find EntityKey when processing server SpawnEntityEvent
    server_entity_to_entity_key: HashMap<TestEntity, EntityKey>,
    /// Track pending client-spawned entities (not yet replicated to server)
    /// Key: ClientKey, Value: EntityKey of pending spawn
    /// This ensures deterministic lookup and enforces "one pending spawn per client" assumption
    pending_client_spawns: HashMap<ClientKey, EntityKey>,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_entity_key: 1,
            entity_map: HashMap::new(),
            client_entity_to_entity_key: HashMap::new(),
            server_entity_to_entity_key: HashMap::new(),
            pending_client_spawns: HashMap::new(),
        }
    }

    /// Allocate a new EntityKey and create its record.
    ///
    /// The record starts in a "pending" state with no entities registered.
    /// Must call `register_server_entity()` or `register_client_entity()` before
    /// the entity is usable. This is enforced by mutate phase semantics.
    pub fn allocate_entity_key(&mut self) -> EntityKey {
        let key = EntityKey::new(self.next_entity_key);
        self.next_entity_key += 1;
        self.entity_map.insert(key, EntityKeyRecord::new());
        key
    }

    /// Get or create the record for an EntityKey
    fn get_or_create_record(&mut self, entity_key: &EntityKey) -> &mut EntityKeyRecord {
        self.entity_map
            .entry(*entity_key)
            .or_insert_with(EntityKeyRecord::new)
    }

    /// Register LocalEntity mapping for server-spawned entities.
    ///
    /// Used when client entity isn't available yet. Client entity registration happens later
    /// when the client receives SpawnEntityEvent.
    pub fn register_client_local_entity_mapping(
        &mut self,
        entity_key: &EntityKey,
        client_key: &ClientKey,
        local_entity: &LocalEntity,
    ) {
        let record = self.get_or_create_record(entity_key);
        record
            .client_local_entities
            .insert(*client_key, *local_entity);
        self.client_entity_to_entity_key
            .insert((*client_key, *local_entity), *entity_key);
    }

    /// Register a server host world entity for an EntityKey.
    pub fn register_server_entity(&mut self, entity_key: &EntityKey, entity: &TestEntity) {
        let record = self.get_or_create_record(entity_key);
        record.server_entity = Some(*entity);
        self.server_entity_to_entity_key
            .insert(*entity, *entity_key);
        // Remove from pending if this was a client-spawned entity that just replicated to server
        self.pending_client_spawns.retain(|_, k| k != entity_key);
    }

    /// Register a client's TestEntity and LocalEntity mapping for an EntityKey.
    ///
    /// Stores both the TestEntity and the reverse LocalEntity -> EntityKey mapping.
    ///
    /// If this is a client-spawned entity (no server entity yet), it is tracked in
    /// `pending_client_spawns` for deterministic lookup when the server receives the spawn event.
    ///
    /// Enforces "one pending per client": if multiple client entities are registered for the same
    /// client before server replication, only the most recent is tracked as pending.
    pub fn register_client_entity(
        &mut self,
        entity_key: &EntityKey,
        client_key: &ClientKey,
        entity: &TestEntity,
        local_entity: &LocalEntity,
    ) {
        let record = self.get_or_create_record(entity_key);
        let was_pending = record.server_entity.is_none();
        record.client_entities.insert(*client_key, *entity);
        record
            .client_local_entities
            .insert(*client_key, *local_entity);
        self.client_entity_to_entity_key
            .insert((*client_key, *local_entity), *entity_key);

        // Track pending client-spawned entities (no server entity yet)
        if was_pending {
            if !self.pending_client_spawns.contains_key(client_key) {
                self.pending_client_spawns.insert(*client_key, *entity_key);
            } else {
                // Verify idempotency: same EntityKey
                let existing = self.pending_client_spawns.get(client_key);
                assert_eq!(
                    existing,
                    Some(entity_key),
                    "Client {:?} already has a pending spawn for different EntityKey {:?}, cannot register {:?}",
                    client_key, existing, entity_key
                );
            }
        }
    }

    /// Get the server host world entity for an EntityKey
    pub fn server_entity(&self, entity_key: &EntityKey) -> Option<TestEntity> {
        self.entity_map.get(entity_key)?.server_entity
    }

    /// Get a client's TestEntity for an EntityKey
    pub fn client_entity(
        &self,
        entity_key: &EntityKey,
        client_key: &ClientKey,
    ) -> Option<TestEntity> {
        self.entity_map
            .get(entity_key)?
            .client_entities
            .get(client_key)
            .copied()
    }

    /// Look up EntityKey from a client's LocalEntity.
    ///
    /// Returns None if the mapping doesn't exist yet (entity hasn't been replicated to that client).
    pub fn entity_key_for_client_entity(
        &self,
        client_key: &ClientKey,
        local_entity: &LocalEntity,
    ) -> Option<EntityKey> {
        self.client_entity_to_entity_key
            .get(&(*client_key, *local_entity))
            .copied()
    }

    /// Look up EntityKey from a server TestEntity.
    ///
    /// Returns None if the server entity isn't registered yet.
    pub fn entity_key_for_server_entity(&self, server_entity: &TestEntity) -> Option<EntityKey> {
        self.server_entity_to_entity_key
            .get(server_entity)
            .copied()
    }

    /// Look up EntityKey from a client's TestEntity.
    ///
    /// Returns None if the client entity isn't registered yet.
    pub fn entity_key_for_client_test_entity(
        &self,
        client_key: &ClientKey,
        entity: &TestEntity,
    ) -> Option<EntityKey> {
        self.entity_map.iter().find_map(|(key, record)| {
            record
                .client_entities
                .get(client_key)
                .filter(|&e| e == entity)
                .map(|_| *key)
        })
    }

    /// Remove and return EntityKey for a pending client-spawned entity.
    ///
    /// This should be called when resolving a pending spawn (e.g., when the server
    /// receives the corresponding SpawnEntityEvent). The entry is consumed to ensure
    /// it's not matched again.
    ///
    /// Returns None if there is no pending spawn for this client.
    pub fn remove_pending_client_spawn(&mut self, client_key: &ClientKey) -> Option<EntityKey> {
        self.pending_client_spawns.remove(client_key)
    }

    /// Get all EntityKeys that have a server entity registered
    /// Returns iterator of (EntityKey, TestEntity) pairs
    pub fn server_entities_iter(&self) -> impl Iterator<Item = (EntityKey, TestEntity)> + '_ {
        self.entity_map
            .iter()
            .filter_map(|(key, record)| record.server_entity.map(|entity| (*key, entity)))
    }

    /// Get all EntityKeys that have a client entity registered for the given ClientKey
    pub fn client_entity_keys(&self, client_key: &ClientKey) -> Vec<EntityKey> {
        self.entity_map
            .iter()
            .filter_map(|(key, record)| record.client_entities.get(client_key).map(|_| *key))
            .collect()
    }

    pub fn unregister_server_entity(&mut self, entity_key: &EntityKey) {
        if let Some(record) = self.entity_map.get_mut(entity_key) {
            if let Some(server_entity) = record.server_entity.take() {
                self.server_entity_to_entity_key.remove(&server_entity);
            }
            if record.server_entity.is_none() && record.client_entities.is_empty() {
                self.entity_map.remove(entity_key);
            }
        }
    }

    pub fn unregister_client_entity(&mut self, entity_key: &EntityKey, client_key: &ClientKey) {
        if let Some(record) = self.entity_map.get_mut(entity_key) {
            record.client_entities.remove(client_key);
            if let Some(local_entity) = record.client_local_entities.remove(client_key) {
                self.client_entity_to_entity_key
                    .remove(&(*client_key, local_entity));
            }
            if record.server_entity.is_none() && record.client_entities.is_empty() {
                self.entity_map.remove(entity_key);
            }
        }
    }
}
