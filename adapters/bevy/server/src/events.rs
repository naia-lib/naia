use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};
use naia_server::{Events, NaiaServerError, User, UserKey};

pub use naia_bevy_shared::events::{
    DespawnEntityEvent, InsertComponentEvents, RemoveComponentEvents, SpawnEntityEvent,
};

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

// UpdateComponentEvents
pub struct UpdateComponentEvents {
    inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>,
}

impl UpdateComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<Entity> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            let mut output = Vec::new();
            for (_tick, entity) in components {
                output.push(*entity);
            }
            return output;
        }

        return Vec::new();
    }
}
