use std::{any::Any, collections::HashMap};

use bevy_ecs::{entity::Entity, prelude::Event};

use naia_bevy_shared::{Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Request, ResponseSendKey, Tick};
use naia_server::{shared::GlobalRequestResponseId, Events, NaiaServerError, User, UserKey};

// ConnectEvent
#[derive(Event)]
pub struct ConnectEvent(pub UserKey);

// DisconnectEvent
#[derive(Event)]
pub struct DisconnectEvent(pub UserKey, pub User);

// ErrorEvent
#[derive(Event)]
pub struct ErrorEvent(pub NaiaServerError);

// TickEvent
#[derive(Event)]
pub struct TickEvent(pub Tick);

// AuthEvents
#[derive(Event)]
pub struct AuthEvents {
    inner: HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>,
}

impl<E: Copy> From<&mut Events<E>> for AuthEvents {
    fn from(events: &mut Events<E>) -> Self {
        Self {
            inner: events.take_auths(),
        }
    }
}

impl AuthEvents {
    pub fn read<M: Message>(&self) -> Vec<(UserKey, M)> {
        let message_kind = MessageKind::of::<M>();

        if let Some(messages) = self.inner.get(&message_kind) {
            return convert_messages(messages);
        }

        Vec::new()
    }
}

// MessageEvents
#[derive(Event)]
pub struct MessageEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>,
}

impl<E: Copy> From<&mut Events<E>> for MessageEvents {
    fn from(events: &mut Events<E>) -> Self {
        Self {
            inner: events.take_messages(),
        }
    }
}

impl MessageEvents {
    pub fn read<C: Channel, M: Message>(&self) -> Vec<(UserKey, M)> {
        let channel_kind = ChannelKind::of::<C>();
        if let Some(message_map) = self.inner.get(&channel_kind) {
            let message_kind = MessageKind::of::<M>();
            if let Some(messages) = message_map.get(&message_kind) {
                return convert_messages(messages);
            }
        }

        Vec::new()
    }
}

fn convert_messages<M: Message>(
    boxed_list: &Vec<(UserKey, MessageContainer)>,
) -> Vec<(UserKey, M)> {
    let mut output_list: Vec<(UserKey, M)> = Vec::new();

    for (user_key, message) in boxed_list {
        let message: M = Box::<dyn Any + 'static>::downcast::<M>(message.clone().to_boxed_any())
            .ok()
            .map(|boxed_m| *boxed_m)
            .unwrap();
        output_list.push((*user_key, message));
    }

    output_list
}

// RequestEvents
#[derive(Event)]
pub struct RequestEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, GlobalRequestResponseId, MessageContainer)>>>,
}

impl<E: Copy> From<&mut Events<E>> for RequestEvents {
    fn from(events: &mut Events<E>) -> Self {
        Self {
            inner: events.take_requests(),
        }
    }
}

impl RequestEvents {
    pub fn read<C: Channel, Q: Request>(&self) -> Vec<(UserKey, ResponseSendKey<Q::Response>, Q)> {
        let channel_kind = ChannelKind::of::<C>();
        let Some(request_map) = self.inner.get(&channel_kind) else {
            return Vec::new();
        };
        let message_kind = MessageKind::of::<Q>();
        let Some(requests) = request_map.get(&message_kind) else {
            return Vec::new();
        };

            let mut output_list: Vec<(UserKey, ResponseSendKey<Q::Response>, Q)> = Vec::new();

            for (user_key, global_response_id, request) in requests {
                let message: Q = Box::<dyn Any + 'static>::downcast::<Q>(request.clone().to_boxed_any())
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .unwrap();

                let response_send_key = ResponseSendKey::new(*global_response_id);

                output_list.push((*user_key, response_send_key, message));
            }

            return output_list;
    }
}

// SpawnEntityEvent
#[derive(Event)]
pub struct SpawnEntityEvent(pub UserKey, pub Entity);

// DespawnEntityEvent
#[derive(Event)]
pub struct DespawnEntityEvent(pub UserKey, pub Entity);

// PublishEntityEvent
#[derive(Event)]
pub struct PublishEntityEvent(pub UserKey, pub Entity);

// UnpublishEntityEvent
#[derive(Event)]
pub struct UnpublishEntityEvent(pub UserKey, pub Entity);

// InsertComponentEvent
#[derive(Event, Clone)]
pub struct InsertComponentEvents {
    inner: HashMap<ComponentKind, Vec<(UserKey, Entity)>>,
}

impl InsertComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(UserKey, Entity)>>) -> Self {
        Self { inner }
    }
    pub fn read<C: Replicate>(&self) -> Vec<(UserKey, Entity)> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            return components.clone();
        }

        return Vec::new();
    }
}

// UpdateComponentEvents
#[derive(Event)]
pub struct UpdateComponentEvents {
    inner: HashMap<ComponentKind, Vec<(UserKey, Entity)>>,
}

impl UpdateComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(UserKey, Entity)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<(UserKey, Entity)> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            return components.clone();
        }

        return Vec::new();
    }
}

// RemoveComponentEvents
#[derive(Event)]
pub struct RemoveComponentEvents {
    inner: HashMap<ComponentKind, Vec<(UserKey, Entity, Box<dyn Replicate>)>>,
}

impl RemoveComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(UserKey, Entity, Box<dyn Replicate>)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<(UserKey, Entity, C)> {
        let mut output = Vec::new();

        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            for (user_key, entity, boxed_component) in components {
                let boxed_any = boxed_component.copy_to_box().to_boxed_any();
                let component: C = Box::<dyn Any + 'static>::downcast::<C>(boxed_any)
                    .ok()
                    .map(|boxed_c| *boxed_c)
                    .unwrap();
                output.push((*user_key, *entity, component));
            }
        }

        output
    }
}
