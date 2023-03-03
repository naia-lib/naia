use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use naia_client::{Events, NaiaClientError};

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};

pub use naia_bevy_shared::events::{
    DespawnEntityEvent, InsertComponentEvents, RemoveComponentEvents, SpawnEntityEvent,
};

// ConnectEvent
pub struct ConnectEvent;

// DisconnectEvent
pub struct DisconnectEvent;

// RejectEvent
pub struct RejectEvent;

// ErrorEvent
pub struct ErrorEvent(pub NaiaClientError);

// MessageEvents
pub struct MessageEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>>,
}

impl From<&mut Events<Entity>> for MessageEvents {
    fn from(events: &mut Events<Entity>) -> Self {
        Self {
            inner: events.take_messages(),
        }
    }
}

impl MessageEvents {
    pub fn read<C: Channel, M: Message>(&self) -> Vec<M> {
        let mut output = Vec::new();

        let channel_kind = ChannelKind::of::<C>();
        if let Some(message_map) = self.inner.get(&channel_kind) {
            let message_kind = MessageKind::of::<M>();
            if let Some(messages) = message_map.get(&message_kind) {
                for boxed_message in messages {
                    let boxed_any = boxed_message.clone().to_boxed_any();
                    let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                        .ok()
                        .map(|boxed_m| *boxed_m)
                        .unwrap();
                    output.push(message);
                }
            }
        }

        output
    }
}

// ClientTickEvent
pub struct ClientTickEvent(pub Tick);

// ServerTickEvent
pub struct ServerTickEvent(pub Tick);

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
