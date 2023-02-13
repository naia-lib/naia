use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use naia_client::{Events, NaiaClientError};

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageKind, Replicate, Tick,
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
    inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>,
}

impl From<&mut Events<Entity>> for RemoveComponentEvents {
    fn from(events: &mut Events<Entity>) -> Self {
        Self {
            inner: events.take_removes(),
        }
    }
}

impl RemoveComponentEvents {
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
