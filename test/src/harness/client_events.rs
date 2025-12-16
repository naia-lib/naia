use std::collections::HashMap;

use naia_client::{NaiaClientError, TickEvents, WorldEvents};
use naia_shared::{ChannelKind, ComponentKind, GlobalResponseId, MessageContainer, MessageKind, Replicate, Tick};

use crate::{Scenario, TestEntity};
use crate::harness::{EntityKey, ClientKey};

pub(crate) struct ClientEvents {
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
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                spawns.push(entity_key);
            }
        }

        let mut despawns = Vec::new();
        for entity in world_events.read::<naia_client::DespawnEntityEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                despawns.push(entity_key);
            }
        }

        let mut publishes = Vec::new();
        for entity in world_events.read::<naia_client::PublishEntityEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                publishes.push(entity_key);
            }
        }

        let mut unpublishes = Vec::new();
        for entity in world_events.read::<naia_client::UnpublishEntityEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                unpublishes.push(entity_key);
            }
        }

        let mut auth_grants = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthGrantedEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                auth_grants.push(entity_key);
            }
        }

        let mut auth_denies = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthDeniedEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                auth_denies.push(entity_key);
            }
        }

        let mut auth_resets = Vec::new();
        for entity in world_events.read::<naia_client::EntityAuthResetEvent>() {
            if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                auth_resets.push(entity_key);
            }
        }

        let mut inserts = HashMap::new();
        for (component_kind, entities) in world_events.take_inserts().unwrap_or_default() {
            let mut entity_keys = Vec::new();
            for entity in entities {
                if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
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
                if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
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
                if let Some(entity_key) = scenario.register_client_entity_event(&client_key, &entity) {
                    entity_keys.push((tick, entity_key));
                }
            }
            if !entity_keys.is_empty() {
                updates.insert(component_kind, entity_keys);
            }
        }

        // Extract connection/rejection/disconnection events (they don't have entity data)
        let connections: Vec<()> = world_events.read::<naia_client::ConnectEvent>().map(|_| ()).collect();
        let rejections: Vec<()> = world_events.read::<naia_client::RejectEvent>().map(|_| ()).collect();
        let disconnections: Vec<()> = world_events.read::<naia_client::DisconnectEvent>().map(|_| ()).collect();
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
}

// ClientEvent trait
pub trait ClientEvent {
    type Iter: Iterator;
    type Item;

    fn iter(events: &mut ClientEvents) -> Self::Iter;
    fn has(events: &ClientEvents) -> bool;
}

// ConnectEvent
pub struct ConnectEvent;
impl ClientEvent for ConnectEvent {
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
pub struct RejectEvent;
impl ClientEvent for RejectEvent {
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
pub struct DisconnectEvent;
impl ClientEvent for DisconnectEvent {
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
pub struct ErrorEvent;
impl ClientEvent for ErrorEvent {
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
pub struct SpawnEntityEvent;
impl ClientEvent for SpawnEntityEvent {
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
pub struct DespawnEntityEvent;
impl ClientEvent for DespawnEntityEvent {
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
pub struct PublishEntityEvent;
impl ClientEvent for PublishEntityEvent {
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
pub struct UnpublishEntityEvent;
impl ClientEvent for UnpublishEntityEvent {
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
pub struct EntityAuthGrantedEvent;
impl ClientEvent for EntityAuthGrantedEvent {
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
pub struct EntityAuthDeniedEvent;
impl ClientEvent for EntityAuthDeniedEvent {
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
pub struct EntityAuthResetEvent;
impl ClientEvent for EntityAuthResetEvent {
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
pub struct ServerTickEvent;
impl ClientEvent for ServerTickEvent {
    type Iter = std::vec::IntoIter<Tick>;
    type Item = Tick;

    fn iter(events: &mut ClientEvents) -> Self::Iter {
        std::mem::take(&mut events.server_ticks).into_iter()
    }

    fn has(events: &ClientEvents) -> bool {
        !events.server_ticks.is_empty()
    }
}