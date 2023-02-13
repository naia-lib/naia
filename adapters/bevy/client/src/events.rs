use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use naia_client::{
    shared::{Channel, ChannelKind, ComponentKind, Message, MessageKind, Replicate, Tick},
    Events, NaiaClientError,
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
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<Box<dyn Message>>>>,
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
                    let boxed_any = boxed_message.clone_box().to_boxed_any();
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

// SpawnEntityEvent
pub struct SpawnEntityEvent(pub Entity);

// DespawnEntityEvent
pub struct DespawnEntityEvent(pub Entity);

// InsertComponentEvent
pub struct InsertComponentEvent(pub Entity, pub ComponentKind);

// UpdateComponentEvent
pub struct UpdateComponentEvent(pub Tick, pub Entity, pub ComponentKind);

// RemoveComponentEvents
pub struct RemoveComponentEvents {
    inner: HashMap<ComponentKind, Vec<Box<dyn Replicate>>>,
}
