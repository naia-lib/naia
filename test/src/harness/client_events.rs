use std::collections::HashMap;

use log::{debug, warn};

use naia_client::{NaiaClientError, TickEvents, WorldEvents};
use naia_shared::{
    ChannelKind, ComponentKind, GlobalResponseId, LocalEntity, MessageContainer, MessageKind,
    OwnedLocalEntity, Replicate, Tick, WorldRefType,
};

use crate::{
    harness::{server_events::ServerEvents, ClientKey, EntityKey},
    Scenario, TestEntity,
};

pub struct ClientEvents {
    connections: Vec<()>,
    rejections: Vec<()>,
    disconnections: Vec<()>,
    errors: Vec<NaiaClientError>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>>,
    requests: HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>>,
    spawns: Vec<EntityKey>,
    despawns: Vec<EntityKey>,
    publishes: Vec<EntityKey>,
    unpublishes: Vec<EntityKey>,
    auth_grants: Vec<EntityKey>,
    auth_denies: Vec<EntityKey>,
    auth_resets: Vec<EntityKey>,
    inserts: HashMap<ComponentKind, Vec<EntityKey>>,
    removes: HashMap<ComponentKind, Vec<(EntityKey, Box<dyn Replicate>)>>,
    updates: HashMap<ComponentKind, Vec<(Tick, EntityKey)>>,
    client_ticks: Vec<Tick>,
    server_ticks: Vec<Tick>,
}

impl Default for ClientEvents {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            rejections: Vec::new(),
            disconnections: Vec::new(),
            errors: Vec::new(),
            messages: HashMap::new(),
            requests: HashMap::new(),
            spawns: Vec::new(),
            despawns: Vec::new(),
            publishes: Vec::new(),
            unpublishes: Vec::new(),
            auth_grants: Vec::new(),
            auth_denies: Vec::new(),
            auth_resets: Vec::new(),
            inserts: HashMap::new(),
            removes: HashMap::new(),
            updates: HashMap::new(),
            client_ticks: Vec::new(),
            server_ticks: Vec::new(),
        }
    }
}

impl ClientEvents {
    pub fn new(
        scenario: &mut Scenario,
        client_key: ClientKey,
        mut world_events: WorldEvents<TestEntity>,
        mut tick_events: TickEvents,
    ) -> Self {
        let mut spawns = Vec::new();
        for entity in world_events.read::<naia_client::SpawnEntityEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                spawns.push(entity_key);
            }
        }

        let mut despawns = Vec::new();
        for entity in world_events.read::<naia_client::DespawnEntityEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                despawns.push(entity_key);
            }
        }

        let mut publishes = Vec::new();
        for entity in world_events.read::<naia_client::PublishEntityEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                publishes.push(entity_key);
            }
        }

        let mut unpublishes = Vec::new();
        for entity in world_events.read::<naia_client::UnpublishEntityEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                unpublishes.push(entity_key);
            }
        }

        let mut auth_grants = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthGrantedEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                auth_grants.push(entity_key);
            }
        }

        let mut auth_denies = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthDeniedEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                auth_denies.push(entity_key);
            }
        }

        let mut auth_resets = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthResetEvent>() {
            if let Some(entity_key) = register_client_entity_event(scenario, &client_key, &entity) {
                auth_resets.push(entity_key);
            }
        }

        let mut inserts = HashMap::new();
        for (component_kind, entities) in world_events.take_inserts().unwrap_or_default() {
            let mut entity_keys = Vec::new();
            for entity in entities {
                if let Some(entity_key) =
                    register_client_entity_event(scenario, &client_key, &entity)
                {
                    entity_keys.push(entity_key);
                }
            }
            if !entity_keys.is_empty() {
                inserts.insert(component_kind, entity_keys);
            }
        }

        let mut removes = HashMap::new();
        for (component_kind, entity_data) in world_events.take_removes().unwrap_or_default() {
            let mut entity_keys = Vec::new();
            for (entity, component) in entity_data {
                if let Some(entity_key) =
                    register_client_entity_event(scenario, &client_key, &entity)
                {
                    entity_keys.push((entity_key, component));
                }
            }
            if !entity_keys.is_empty() {
                removes.insert(component_kind, entity_keys);
            }
        }

        let mut updates = HashMap::new();
        for (component_kind, entity_data) in world_events.take_updates().unwrap_or_default() {
            let mut entity_keys = Vec::new();
            for (tick, entity) in entity_data {
                if let Some(entity_key) =
                    register_client_entity_event(scenario, &client_key, &entity)
                {
                    entity_keys.push((tick, entity_key));
                }
            }
            if !entity_keys.is_empty() {
                updates.insert(component_kind, entity_keys);
            }
        }

        // Extract connection/rejection/disconnection events (they don't have entity data)
        let connections: Vec<()> = world_events
            .read::<naia_client::ConnectEvent>()
            .map(|_| ())
            .collect();
        let rejections: Vec<()> = world_events
            .read::<naia_client::RejectEvent>()
            .map(|_| ())
            .collect();
        let disconnections: Vec<()> = world_events
            .read::<naia_client::DisconnectEvent>()
            .map(|_| ())
            .collect();
        let errors: Vec<NaiaClientError> = world_events.read::<naia_client::ErrorEvent>().collect();
        let messages = world_events.take_messages();
        let requests = world_events.take_requests();

        // Extract tick events
        let client_ticks: Vec<Tick> = tick_events.read::<naia_client::ClientTickEvent>().collect();
        let server_ticks: Vec<Tick> = tick_events.read::<naia_client::ServerTickEvent>().collect();

        Self {
            connections,
            rejections,
            disconnections,
            errors,
            messages,
            requests,
            spawns,
            despawns,
            publishes,
            unpublishes,
            auth_grants,
            auth_denies,
            auth_resets,
            inserts,
            removes,
            updates,
            client_ticks,
            server_ticks,
        }
    }

    pub fn read<V: ClientEvent>(&mut self) -> V::Iter {
        V::iter(self)
    }

    pub fn has<V: ClientEvent>(&self) -> bool {
        V::has(self)
    }

    pub fn take_messages_for_channel_and_type(
        &mut self,
        channel_kind: &naia_shared::ChannelKind,
        message_kind: &naia_shared::MessageKind,
    ) -> Vec<naia_shared::MessageContainer> {
        self.messages
            .get_mut(channel_kind)
            .and_then(|channel_messages| channel_messages.remove(message_kind))
            .unwrap_or_default()
    }

    pub fn take_requests_for_channel_and_type(
        &mut self,
        channel_kind: &naia_shared::ChannelKind,
        message_kind: &naia_shared::MessageKind,
    ) -> Vec<(naia_shared::GlobalResponseId, naia_shared::MessageContainer)> {
        self.requests
            .get_mut(channel_kind)
            .and_then(|channel_requests| channel_requests.remove(message_kind))
            .unwrap_or_default()
    }

    /// Take all insert events for a specific component kind
    pub fn take_inserts_for_component(
        &mut self,
        component_kind: &ComponentKind,
    ) -> Vec<EntityKey> {
        self.inserts
            .get_mut(component_kind)
            .map(|v| std::mem::take(v))
            .unwrap_or_default()
    }

    /// Take all remove events for a specific component kind
    pub fn take_removes_for_component(
        &mut self,
        component_kind: &ComponentKind,
    ) -> Vec<(EntityKey, Box<dyn Replicate>)> {
        self.removes
            .get_mut(component_kind)
            .map(|v| std::mem::take(v))
            .unwrap_or_default()
    }

    /// Take all update events for a specific component kind
    pub fn take_updates_for_component(
        &mut self,
        component_kind: &ComponentKind,
    ) -> Vec<(Tick, EntityKey)> {
        self.updates
            .get_mut(component_kind)
            .map(|v| std::mem::take(v))
            .unwrap_or_default()
    }
}

// ClientEvent trait
pub trait ClientEvent {
    type Iter: Iterator;
    type Item;

    fn iter(events: &mut ClientEvents) -> Self::Iter;
    fn has(events: &ClientEvents) -> bool;
}

// ConnectEvent
pub struct ClientConnectEvent;
impl ClientEvent for ClientConnectEvent {
    type Iter = std::vec::IntoIter<()>;
    type Item = ();

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.connections).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.connections.is_empty()
    }
}

// RejectEvent
pub struct ClientRejectEvent;
impl ClientEvent for ClientRejectEvent {
    type Iter = std::vec::IntoIter<()>;
    type Item = ();

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.rejections).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.rejections.is_empty()
    }
}

// DisconnectEvent
pub struct ClientDisconnectEvent;
impl ClientEvent for ClientDisconnectEvent {
    type Iter = std::vec::IntoIter<()>;
    type Item = ();

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.disconnections).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.disconnections.is_empty()
    }
}

// ErrorEvent
pub struct ClientErrorEvent;
impl ClientEvent for ClientErrorEvent {
    type Iter = std::vec::IntoIter<NaiaClientError>;
    type Item = NaiaClientError;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.errors).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.errors.is_empty()
    }
}

// SpawnEntityEvent
pub struct ClientSpawnEntityEvent;
impl ClientEvent for ClientSpawnEntityEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.spawns).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.spawns.is_empty()
    }
}

// DespawnEntityEvent
pub struct ClientDespawnEntityEvent;
impl ClientEvent for ClientDespawnEntityEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.despawns).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.despawns.is_empty()
    }
}

// PublishEntityEvent
pub struct ClientPublishEntityEvent;
impl ClientEvent for ClientPublishEntityEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.publishes).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.publishes.is_empty()
    }
}

// UnpublishEntityEvent
pub struct ClientUnpublishEntityEvent;
impl ClientEvent for ClientUnpublishEntityEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.unpublishes).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.unpublishes.is_empty()
    }
}

// EntityAuthGrantedEvent
pub struct ClientEntityAuthGrantedEvent;
impl ClientEvent for ClientEntityAuthGrantedEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.auth_grants).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.auth_grants.is_empty()
    }
}

// EntityAuthDeniedEvent
pub struct ClientEntityAuthDeniedEvent;
impl ClientEvent for ClientEntityAuthDeniedEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.auth_denies).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.auth_denies.is_empty()
    }
}

// EntityAuthResetEvent
pub struct ClientEntityAuthResetEvent;
impl ClientEvent for ClientEntityAuthResetEvent {
    type Iter = std::vec::IntoIter<EntityKey>;
    type Item = EntityKey;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.auth_resets).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.auth_resets.is_empty()
    }
}

// ClientTickEvent
pub struct ClientTickEvent;
impl ClientEvent for ClientTickEvent {
    type Iter = std::vec::IntoIter<Tick>;
    type Item = Tick;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.client_ticks).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.client_ticks.is_empty()
    }
}

// ServerTickEvent
pub struct ClientServerTickEvent;
impl ClientEvent for ClientServerTickEvent {
    type Iter = std::vec::IntoIter<Tick>;
    type Item = Tick;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.server_ticks).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.server_ticks.is_empty()
    }
}

/// Register client entity and return EntityKey
pub(crate) fn register_client_entity_event(
    scenario: &mut Scenario,
    client_key: &ClientKey,
    entity: &TestEntity,
) -> Option<EntityKey> {
    let state = scenario.client_state(client_key);

    let world_ref = state.world().proxy();
    if !world_ref.has_entity(entity) {
        return None;
    }
    let client_ref = state.client().entity(world_ref, entity);
    let local_entity = client_ref.local_entity()?;
    // Try to find EntityKey by matching LocalEntity with server entities
    let entity_key = match scenario
        .entity_registry()
        .entity_key_for_client_entity(client_key, &local_entity)
    {
        Some(ek) => ek,
        None => {
            // For server-spawned entities, match by LocalEntity value with server entities
            let local_entity_value = extract_local_entity_value(&local_entity);
            let mut matched_key = None;

            if let Some(user_key) = scenario.client_to_user_key(client_key) {
                let server = scenario.server().as_ref()?;
                let server_entities: Vec<_> =
                    scenario.entity_registry().server_entities_iter().collect();

                for (ek, server_entity) in &server_entities {
                    let world_ref = scenario.server_world_ref();
                    if !world_ref.has_entity(server_entity) {
                        continue;
                    }
                    let server_ref = server.entity(world_ref, server_entity);
                    if let Some(server_local_entity) = server_ref.local_entity(&user_key) {
                        let server_value = extract_local_entity_value(&server_local_entity);
                        if server_value == local_entity_value {
                            matched_key = Some(*ek);
                            break;
                        }
                    }
                }
            }

            match matched_key {
                Some(ek) => {
                    // Verify entity is still in client world before registering
                    let state = scenario.client_state(client_key);
                    let world_ref = state.world().proxy();
                    if !world_ref.has_entity(entity) {
                        return None;
                    }
                    // Register the mapping
                    scenario
                        .entity_registry_mut()
                        .register_client_local_entity_mapping(&ek, client_key, &local_entity);
                    // Register the client entity
                    scenario.entity_registry_mut().register_client_entity(
                        &ek,
                        client_key,
                        entity,
                        &local_entity,
                    );
                    ek
                }
                None => {
                    return None;
                }
            }
        }
    };

    // Register client entity if not already registered
    if scenario
        .entity_registry()
        .client_entity(&entity_key, client_key)
        .is_none()
    {
        scenario.entity_registry_mut().register_client_entity(
            &entity_key,
            client_key,
            entity,
            &local_entity,
        );
    }

    Some(entity_key)
}

/// Process spawn events to match and register client-spawned entities with server entities
pub(crate) fn process_spawn_events(
    scenario: &mut Scenario,
    _server_events: &mut ServerEvents,
    client_events_map: &mut HashMap<ClientKey, ClientEvents>,
) {
    // Collect client spawn events (client-side SpawnEntityEvent means client spawned it)
    let mut spawns_to_register = Vec::new();
    for (client_key, _client_events) in client_events_map.iter_mut() {
        // Check for new client spawns in this tick
        // These are entities the client created that we need to match with server
        let state = scenario
            .clients()
            .get(client_key)
            .expect("client not found");
        let client = state.client();

        // Get all entities that exist on this client
        let entities = {
            let world_ref = state.world().proxy();
            client.entities(&world_ref)
        };

        for entity in entities {
            let world_ref = state.world().proxy();
            let client_ref = client.entity(world_ref, &entity);
            if let Some(local_entity) = client_ref.local_entity() {
                // Check if this entity is already registered in our registry
                if scenario
                    .entity_registry()
                    .entity_key_for_client_entity(client_key, &local_entity)
                    .is_none()
                {
                    // This is a new client entity - add to list for matching
                    spawns_to_register.push((*client_key, local_entity, entity));
                }
            }
        }
    }

    register_client_spawns(scenario, spawns_to_register);
}

/// Process despawn events to unregister entities from the registry
pub(crate) fn process_despawn_events(
    scenario: &mut Scenario,
    server_events: &mut ServerEvents,
    client_events_map: &mut HashMap<ClientKey, ClientEvents>,
) {
    // 1. Process server despawn events
    for (_client_key, entity_key) in server_events.despawns() {
        scenario
            .entity_registry_mut()
            .unregister_server_entity(entity_key);
    }

    // 2. Process client despawn events
    for (client_key, client_events) in client_events_map.iter_mut() {
        for entity_key in client_events.read::<crate::ClientDespawnEntityEvent>() {
            scenario
                .entity_registry_mut()
                .unregister_client_entity(&entity_key, client_key);
        }
    }
}

/// Register client spawns by matching LocalEntity values with server entities.
fn register_client_spawns(
    scenario: &mut Scenario,
    spawns_to_register: Vec<(ClientKey, LocalEntity, TestEntity)>,
) {
    for (client_key, local_entity, client_entity) in spawns_to_register {
        let local_entity_value = extract_local_entity_value(&local_entity);

        // Skip if already registered
        if let Some(existing_key) = scenario
            .entity_registry()
            .entity_key_for_client_entity(&client_key, &local_entity)
        {
            debug!(
                "Skipping already-registered client entity {:?} for client {:?}",
                existing_key, client_key
            );
            continue;
        }

        // Match EntityKey by LocalEntity value
        let (entity_key, server_entities_count) = {
            let server_entities: Vec<_> =
                scenario.entity_registry().server_entities_iter().collect();
            let count = server_entities.len();

            let mut matched_key = None;

            if let Some(user_key) = scenario.client_to_user_key(&client_key) {
                let server = scenario.server().as_ref().expect("server not started");

                for (ek, server_entity) in &server_entities {
                    let world_ref = scenario.server_world_ref();
                    if !world_ref.has_entity(server_entity) {
                        continue;
                    }
                    let server_ref = server.entity(world_ref, server_entity);
                    if let Some(server_local_entity) = server_ref.local_entity(&user_key) {
                        let server_value = extract_local_entity_value(&server_local_entity);
                        if server_value == local_entity_value {
                            debug!(
                                "Matched LocalEntity value {} to server entity {:?}",
                                local_entity_value, ek
                            );
                            matched_key = Some(*ek);
                            break;
                        }
                    }
                }
            }

            (matched_key, count)
        };

        if let Some(entity_key) = entity_key {
            scenario.entity_registry_mut().register_client_entity(
                &entity_key,
                &client_key,
                &client_entity,
                &local_entity,
            );
        } else {
            warn!(
                "Phase D: Failed to resolve EntityKey for client {:?} with LocalEntity value {} (checked {} server entities). \
                 This may indicate a mapping lifecycle violation - entity should resolve in a future tick.",
                client_key, local_entity_value, server_entities_count
            );
        }
    }
}

/// Extract the comparable value from a LocalEntity.
///
/// This relies on Naia's current internal representation where `LocalEntity` wraps
/// an `OwnedLocalEntity` enum with a `u16` value. The server and client share the
/// same value for the same user's view of an entity.
///
/// # TODO: Brittleness
///
/// This assumes Naia's internal representation. If Naia changes how `LocalEntity`
/// is represented or provides a public API for comparison, this should be updated.
/// Consider contributing a public comparison API to the naia crate.
fn extract_local_entity_value(local_entity: &LocalEntity) -> u16 {
    let owned: OwnedLocalEntity = (*local_entity).into();
    match owned {
        OwnedLocalEntity::Host(v) | OwnedLocalEntity::Remote(v) => v,
    }
}
