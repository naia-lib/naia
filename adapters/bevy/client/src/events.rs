use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{entity::Entity, prelude::Event};

use naia_client::{Events, NaiaClientError};

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};

// ConnectEvent
#[derive(Event)]
pub struct ConnectEvent<T> {
    phantom_t: PhantomData<T>,
}

impl<T> ConnectEvent<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// DisconnectEvent
#[derive(Event)]
pub struct DisconnectEvent<T> {
    phantom_t: PhantomData<T>,
}

impl<T> DisconnectEvent<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// RejectEvent
#[derive(Event)]
pub struct RejectEvent<T> {
    phantom_t: PhantomData<T>,
}

impl<T> RejectEvent<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// ErrorEvent
#[derive(Event)]
pub struct ErrorEvent<T> {
    pub err: NaiaClientError,
    phantom_t: PhantomData<T>,
}

impl<T> ErrorEvent<T> {
    pub fn new(err: NaiaClientError) -> Self {
        Self {
            err,
            phantom_t: PhantomData,
        }
    }
}

// MessageEvents
#[derive(Event)]
pub struct MessageEvents<T> {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>>,
    phantom_t: PhantomData<T>,
}

impl<T> From<&mut Events<Entity>> for MessageEvents<T> {
    fn from(events: &mut Events<Entity>) -> Self {
        Self {
            inner: events.take_messages(),
            phantom_t: PhantomData,
        }
    }
}

impl<T> MessageEvents<T> {
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
pub struct ClientTickEvent<T> {
    pub tick: Tick,
    phantom_t: PhantomData<T>,
}

impl<T> ClientTickEvent<T> {
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            phantom_t: PhantomData,
        }
    }
}

// ServerTickEvent
#[derive(Event)]
pub struct ServerTickEvent<T> {
    pub tick: Tick,
    phantom_t: PhantomData<T>,
}

impl<T> ServerTickEvent<T> {
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            phantom_t: PhantomData,
        }
    }
}

// SpawnEntityEvent
#[derive(Event)]
pub struct SpawnEntityEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> SpawnEntityEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// DespawnEntityEvent
#[derive(Event)]
pub struct DespawnEntityEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> DespawnEntityEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// PublishEntityEvent
#[derive(Event)]
pub struct PublishEntityEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> PublishEntityEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// UnpublishEntityEvent
#[derive(Event)]
pub struct UnpublishEntityEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> UnpublishEntityEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthGrantedEvent
#[derive(Event)]
pub struct EntityAuthGrantedEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthGrantedEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthDeniedEvent
#[derive(Event)]
pub struct EntityAuthDeniedEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthDeniedEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthResetEvent
#[derive(Event)]
pub struct EntityAuthResetEvent<T> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthResetEvent<T> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// InsertComponentEvent
#[derive(Event, Clone)]
pub struct InsertComponentEvents<T> {
    inner: HashMap<ComponentKind, Vec<Entity>>,
    phantom_t: PhantomData<T>,
}

impl<T> InsertComponentEvents<T> {
    pub fn new(inner: HashMap<ComponentKind, Vec<Entity>>) -> Self {
        Self {
            inner,
            phantom_t: PhantomData,
        }
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
#[derive(Event, Clone)]
pub struct UpdateComponentEvents<T> {
    inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>,
    phantom_t: PhantomData<T>,
}

impl<T> UpdateComponentEvents<T> {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Tick, Entity)>>) -> Self {
        Self {
            inner,
            phantom_t: PhantomData,
        }
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
pub struct RemoveComponentEvents<T> {
    inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>,
    phantom_t: PhantomData<T>,
}

impl<T> RemoveComponentEvents<T> {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>) -> Self {
        Self {
            inner,
            phantom_t: PhantomData,
        }
    }

    pub fn clone_new(&self) -> Self {
        let mut output = HashMap::new();

        for (key, value) in self.inner.iter() {
            let mut list = Vec::new();

            for item in value {
                list.push((item.0, item.1.copy_to_box()));
            }

            output.insert(*key, list);
        }

        Self::new(output)
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
