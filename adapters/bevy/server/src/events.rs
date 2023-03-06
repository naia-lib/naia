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
        let message_kind = MessageKind::of::<M>();

        if let Some(messages) = self.inner.get(&message_kind) {
            return convert_messages(messages);
        }

        Vec::new()
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

// SpawnEntityEvent
pub struct SpawnEntityEvent(pub UserKey, pub Entity);

// DespawnEntityEvent
pub struct DespawnEntityEvent(pub UserKey, pub Entity);

// InsertComponentEvent
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
