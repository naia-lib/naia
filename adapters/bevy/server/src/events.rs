use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};
use naia_server::{Events, NaiaServerError, User, UserKey};

// ConnectEvent
pub struct ConnectEvent(pub UserKey);

// DisconnectEvent
pub struct DisconnectEvent(pub UserKey, pub User);

// ErrorEvent
pub struct ErrorEvent(pub NaiaServerError);

// TickEvent
pub struct TickEvent(pub Tick);

// AuthEvents
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
        let mut output = Vec::new();

        let message_kind = MessageKind::of::<M>();

        if let Some(messages) = self.inner.get(&message_kind) {
            for (user_key, boxed_message) in messages {
                let boxed_any = boxed_message.clone().to_boxed_any();
                let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .unwrap();
                output.push((*user_key, message));
            }
        }

        output
    }
}

// MessageEvents
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
        let mut output = Vec::new();

        let channel_kind = ChannelKind::of::<C>();
        if let Some(message_map) = self.inner.get(&channel_kind) {
            let message_kind = MessageKind::of::<M>();
            if let Some(messages) = message_map.get(&message_kind) {
                for (user_key, boxed_message) in messages {
                    let boxed_any = boxed_message.clone().to_boxed_any();
                    let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                        .ok()
                        .map(|boxed_m| *boxed_m)
                        .unwrap();
                    output.push((*user_key, message));
                }
            }
        }

        output
    }
}

// SpawnEntityEvent
pub struct SpawnEntityEvent(pub Entity);

// DespawnEntityEvent
pub struct DespawnEntityEvent(pub Entity);

// InsertComponentEvent
pub struct InsertComponentEvents {
    inner: HashMap<ComponentKind, Vec<Entity>>,
}

impl InsertComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<Entity>>) -> Self {
        Self { inner }
    }
    pub fn read<C: Replicate>(&self) -> Vec<Entity> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            return components.clone();
        }

        return Vec::new();
    }
}

// UpdateComponentEvents
pub struct UpdateComponentEvents {
    inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>,
}

impl UpdateComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<(Tick, Entity)> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            return components.clone();
        }

        return Vec::new();
    }
}

// RemoveComponentEvents
pub struct RemoveComponentEvents {
    inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>,
}

impl RemoveComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<(Entity, C)> {
        let mut output = Vec::new();

        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            for (entity, boxed_component) in components {
                let boxed_any = boxed_component.copy_to_box().to_boxed_any();
                let component: C = Box::<dyn Any + 'static>::downcast::<C>(boxed_any)
                    .ok()
                    .map(|boxed_c| *boxed_c)
                    .unwrap();
                output.push((*entity, component));
            }
        }

        output
    }
}
