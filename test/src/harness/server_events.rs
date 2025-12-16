use std::collections::HashMap;

use naia_server::{NaiaServerError, TickEvents, Events, UserKey};
use naia_shared::{ChannelKind, ComponentKind, GlobalResponseId, MessageContainer, MessageKind, Replicate, Tick, Message};

use crate::{ClientKey, Scenario, TestEntity};
use crate::harness::entity_registry::EntityRegistry;

pub(crate) struct ServerEvents {
    auths: HashMap<MessageKind, Vec<(ClientKey, MessageContainer)>>,
    connections: Vec<ClientKey>,
    disconnections: Vec<ClientKey>,
    errors: Vec<NaiaServerError>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<(ClientKey, MessageContainer)>>>,
    requests: HashMap<
        ChannelKind,
        HashMap<MessageKind, Vec<(ClientKey, GlobalResponseId, MessageContainer)>>,
    >,
    spawns: Vec<(ClientKey, EntityKey)>,
    despawns: Vec<(ClientKey, EntityKey)>,
    publishes: Vec<(ClientKey, EntityKey)>,
    unpublishes: Vec<(ClientKey, EntityKey)>,
    delegates: Vec<(ClientKey, EntityKey)>,
    auth_grants: Vec<(ClientKey, EntityKey)>,
    auth_resets: Vec<EntityKey>,
    inserts: HashMap<ComponentKind, Vec<(ClientKey, EntityKey)>>,
    removes: HashMap<ComponentKind, Vec<(ClientKey, EntityKey, Box<dyn Replicate>)>>,
    updates: HashMap<ComponentKind, Vec<(ClientKey, EntityKey)>>,
    ticks: Vec<Tick>,
}

use crate::harness::EntityKey;

impl ServerEvents {
    pub fn new(
        scenario: &mut Scenario,
        auths: HashMap<MessageKind, Vec<(naia_server::UserKey, MessageContainer)>>,
        mut tick_events: TickEvents,
        mut events: Events<TestEntity>,
    ) -> Self {
        // Convert main events: auths (use helper method)
        let mut client_auths = HashMap::new();
        for (message_kind, user_auths) in auths {
            let mut client_auth_list = Vec::new();
            for (user_key, message_container) in user_auths {
                if let Some((client_key, auth_container)) = scenario.register_auth_event(user_key, message_container) {
                    client_auth_list.push((client_key, auth_container));
                }
            }
            if !client_auth_list.is_empty() {
                client_auths.insert(message_kind, client_auth_list);
            }
        }

        // Convert main events: connections (from world_events in combined Events)
        let mut connections = Vec::new();
        for user_key in events.read::<naia_server::ConnectEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                connections.push(client_key);
            }
        }

        // Convert main events: errors (from world_events in combined Events)
        let errors: Vec<NaiaServerError> = events.read::<naia_server::ErrorEvent>().collect();

        // Convert world events: disconnections
        let mut disconnections = Vec::new();
        for (user_key, _addr) in events.read::<naia_server::DisconnectEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                disconnections.push(client_key);
            }
        }

        // Convert world events: messages
        let mut messages = HashMap::new();
        for (channel_kind, channel_messages) in events.take_messages() {
            let mut client_messages = HashMap::new();
            for (message_kind, user_messages) in channel_messages {
                let mut client_message_list = Vec::new();
                for (user_key, message_container) in user_messages {
                    if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                        client_message_list.push((client_key, message_container));
                    }
                }
                if !client_message_list.is_empty() {
                    client_messages.insert(message_kind, client_message_list);
                }
            }
            if !client_messages.is_empty() {
                messages.insert(channel_kind, client_messages);
            }
        }

        // Convert world events: requests
        let mut requests = HashMap::new();
        for (channel_kind, channel_requests) in events.take_requests() {
            let mut client_requests = HashMap::new();
            for (message_kind, user_requests) in channel_requests {
                let mut client_request_list = Vec::new();
                for (user_key, response_id, message_container) in user_requests {
                    if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                        client_request_list.push((client_key, response_id, message_container));
                    }
                }
                if !client_request_list.is_empty() {
                    client_requests.insert(message_kind, client_request_list);
                }
            }
            if !client_requests.is_empty() {
                requests.insert(channel_kind, client_requests);
            }
        }

        // Convert world events: spawns (use helper method)
        let mut spawns = Vec::new();
        for (user_key, server_entity) in events.read::<naia_server::SpawnEntityEvent>() {
            if let Some((client_key, entity_key)) = scenario.register_spawn_entity(user_key, server_entity) {
                spawns.push((client_key, entity_key));
            }
        }

        // Convert world events: despawns
        let mut despawns = Vec::new();
        for (user_key, entity) in events.read::<naia_server::DespawnEntityEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                    despawns.push((client_key, entity_key));
                }
            }
        }

        // Convert world events: publishes
        let mut publishes = Vec::new();
        for (user_key, entity) in events.read::<naia_server::PublishEntityEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                    publishes.push((client_key, entity_key));
                }
            }
        }

        // Convert world events: unpublishes
        let mut unpublishes = Vec::new();
        for (user_key, entity) in events.read::<naia_server::UnpublishEntityEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                    unpublishes.push((client_key, entity_key));
                }
            }
        }

        // Convert world events: delegates
        let mut delegates = Vec::new();
        for (user_key, entity) in events.read::<naia_server::DelegateEntityEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                    delegates.push((client_key, entity_key));
                }
            }
        }

        // Convert world events: auth_grants
        let mut auth_grants = Vec::new();
        for (user_key, entity) in events.read::<naia_server::EntityAuthGrantEvent>() {
            if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                    auth_grants.push((client_key, entity_key));
                }
            }
        }

        // Convert world events: auth_resets
        let mut auth_resets = Vec::new();
        for entity in events.read::<naia_server::EntityAuthResetEvent>() {
            if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                auth_resets.push(entity_key);
            }
        }

        // Convert world events: inserts
        let mut inserts = HashMap::new();
        if let Some(inserts_data) = events.take_inserts() {
            for (component_kind, entity_data) in inserts_data {
            let mut client_entities = Vec::new();
            for (user_key, entity) in entity_data {
                if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                    if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                        client_entities.push((client_key, entity_key));
                    }
                }
            }
                if !client_entities.is_empty() {
                    inserts.insert(component_kind, client_entities);
                }
            }
        }

        // Convert world events: removes
        let mut removes = HashMap::new();
        if let Some(removes_data) = events.take_removes() {
            for (component_kind, entity_data) in removes_data {
            let mut client_entities = Vec::new();
            for (user_key, entity, component) in entity_data {
                if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                    if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                        client_entities.push((client_key, entity_key, component));
                    }
                }
            }
                if !client_entities.is_empty() {
                    removes.insert(component_kind, client_entities);
                }
            }
        }

        // Convert world events: updates
        let mut updates = HashMap::new();
        if let Some(updates_data) = events.take_updates() {
            for (component_kind, entity_data) in updates_data {
            let mut client_entities = Vec::new();
            for (user_key, entity) in entity_data {
                if let Some(client_key) = scenario.user_to_client_key(&user_key) {
                    if let Some(entity_key) = scenario.entity_registry().entity_key_for_server_entity(&entity) {
                        client_entities.push((client_key, entity_key));
                    }
                }
            }
                if !client_entities.is_empty() {
                    updates.insert(component_kind, client_entities);
                }
            }
        }

        // Extract tick events
        let ticks: Vec<Tick> = tick_events.read::<naia_server::TickEvent>().collect();

        Self {
            auths: client_auths,
            connections,
            disconnections,
            errors,
            messages,
            requests,
            spawns,
            despawns,
            publishes,
            unpublishes,
            delegates,
            auth_grants,
            auth_resets,
            inserts,
            removes,
            updates,
            ticks,
        }
    }

    pub fn read<V: ServerEvent>(&mut self) -> V::Iter {
        V::iter(self)
    }

    pub fn has<V: ServerEvent>(&self) -> bool {
        V::has(self)
    }
}

// ServerEvent trait
pub trait ServerEvent {
    type Iter: Iterator;
    type Item;

    fn iter(events: &mut ServerEvents) -> Self::Iter;
    fn has(events: &ServerEvents) -> bool;
}

// AuthEvent
pub struct AuthEvent<M: Message> {
    _phantom: std::marker::PhantomData<M>,
}

fn read_messages<M: Message>(messages: Vec<(ClientKey, MessageContainer)>) -> Vec<(ClientKey, M)> {
    let mut output_list = Vec::new();
    for (client_key, message_container) in messages {
        // Downcast MessageContainer to concrete message type M (following naia_server pattern)
        let message: M = Box::<dyn std::any::Any + 'static>::downcast::<M>(message_container.to_boxed_any())
            .ok()
            .map(|boxed_m| *boxed_m)
            .unwrap();
        output_list.push((client_key, message));
    }
    output_list
}

impl<M: Message> ServerEvent for AuthEvent<M> {
    type Iter = std::vec::IntoIter<(ClientKey, M)>;
    type Item = (ClientKey, M);

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        let message_kind = MessageKind::of::<M>();
        if let Some(auths) = events.auths.remove(&message_kind) {
            read_messages(auths).into_iter()
        } else {
            Vec::new().into_iter()
        }
    }

    fn has(events: &ServerEvents) -> bool {
        let message_kind = MessageKind::of::<M>();
        events.auths.contains_key(&message_kind)
    }
}

// ConnectEvent
pub struct ConnectEvent;
impl ServerEvent for ConnectEvent {
    type Iter = std::vec::IntoIter<ClientKey>;
    type Item = ClientKey;

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.connections).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.connections.is_empty()
    }
}

// DisconnectEvent (harness version)
pub struct DisconnectEvent;
impl ServerEvent for DisconnectEvent {
    type Iter = std::vec::IntoIter<ClientKey>;
    type Item = ClientKey;

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.disconnections).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.disconnections.is_empty()
    }
}

// ErrorEvent
pub struct ErrorEvent;
impl ServerEvent for ErrorEvent {
    type Iter = std::vec::IntoIter<NaiaServerError>;
    type Item = NaiaServerError;

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.errors).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.errors.is_empty()
    }
}

// SpawnEntityEvent
pub struct SpawnEntityEvent;
impl ServerEvent for SpawnEntityEvent {
    type Iter = std::vec::IntoIter<(ClientKey, EntityKey)>;
    type Item = (ClientKey, EntityKey);

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.spawns).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.spawns.is_empty()
    }
}

// DespawnEntityEvent
pub struct DespawnEntityEvent;
impl ServerEvent for DespawnEntityEvent {
    type Iter = std::vec::IntoIter<(ClientKey, EntityKey)>;
    type Item = (ClientKey, EntityKey);

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.despawns).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.despawns.is_empty()
    }
}

// TickEvent
pub struct TickEvent;
impl ServerEvent for TickEvent {
    type Iter = std::vec::IntoIter<Tick>;
    type Item = Tick;

    fn iter(events: &mut ServerEvents) -> Self::Iter {
        std::mem::take(&mut events.ticks).into_iter()
    }

    fn has(events: &ServerEvents) -> bool {
        !events.ticks.is_empty()
    }
}