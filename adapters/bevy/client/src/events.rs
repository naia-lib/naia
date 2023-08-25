use std::{any::Any, collections::HashMap};

use bevy_ecs::{entity::Entity, prelude::Event};

use naia_client::{Events, NaiaClientError};

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};

// ConnectEvent
#[derive(Event)]
pub struct ConnectEvent;

// DisconnectEvent
#[derive(Event)]
pub struct DisconnectEvent;

// RejectEvent
#[derive(Event)]
pub struct RejectEvent;

// ErrorEvent
#[derive(Event)]
pub struct ErrorEvent(pub NaiaClientError);

// MessageEvents
#[derive(Event)]
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
#[derive(Event)]
pub struct ClientTickEvent(pub Tick);

// ServerTickEvent
#[derive(Event)]
pub struct ServerTickEvent(pub Tick);

// SpawnEntityEvent
#[derive(Event)]
pub struct SpawnEntityEvent(pub Entity);

// DespawnEntityEvent
#[derive(Event)]
pub struct DespawnEntityEvent(pub Entity);

// PublishEntityEvent
#[derive(Event)]
pub struct PublishEntityEvent(pub Entity);

// UnpublishEntityEvent
#[derive(Event)]
pub struct UnpublishEntityEvent(pub Entity);

// EntityAuthGrantedEvent
#[derive(Event)]
pub struct EntityAuthGrantedEvent(pub Entity);

// EntityAuthDeniedEvent
#[derive(Event)]
pub struct EntityAuthDeniedEvent(pub Entity);

// EntityAuthResetEvent
#[derive(Event)]
pub struct EntityAuthResetEvent(pub Entity);

// InsertComponentEvent
#[derive(Event)]
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
#[derive(Event)]
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
#[derive(Event)]
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
