use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    event::{Event, EventReader},
    resource::Resource,
    system::SystemState,
};

use naia_client::{shared::GlobalResponseId, NaiaClientError, WorldEvents};

use naia_bevy_shared::{
    Channel, ChannelKind, Message, MessageContainer, MessageKind, ReplicateBundle, Request,
    ResponseSendKey, Tick,
};

use crate::Replicate;

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

impl<T> From<&mut WorldEvents<Entity>> for MessageEvents<T> {
    fn from(events: &mut WorldEvents<Entity>) -> Self {
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

// RequestEvents
#[derive(Event)]
pub struct RequestEvents<T> {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>>,
    phantom_t: PhantomData<T>,
}

impl<T> From<&mut WorldEvents<Entity>> for RequestEvents<T> {
    fn from(events: &mut WorldEvents<Entity>) -> Self {
        Self {
            inner: events.take_requests(),
            phantom_t: PhantomData,
        }
    }
}

impl<T> RequestEvents<T> {
    pub fn read<C: Channel, Q: Request>(&self) -> Vec<(ResponseSendKey<Q::Response>, Q)> {
        let mut output = Vec::new();

        let channel_kind = ChannelKind::of::<C>();
        let Some(request_map) = self.inner.get(&channel_kind) else {
            return Vec::new();
        };
        let message_kind = MessageKind::of::<Q>();
        let Some(requests) = request_map.get(&message_kind) else {
            return Vec::new();
        };
        for (global_response_id, boxed_message) in requests {
            let boxed_any = boxed_message.clone().to_boxed_any();
            let request: Q = Box::<dyn Any + 'static>::downcast::<Q>(boxed_any)
                .ok()
                .map(|boxed_m| *boxed_m)
                .unwrap();
            let response_send_key = ResponseSendKey::new(*global_response_id);
            output.push((response_send_key, request));
        }

        output
    }
}

// ClientTickEventReader
#[derive(Resource)]
pub(crate) struct CachedClientTickEventsState<T: Send + Sync + 'static> {
    pub(crate) event_state: SystemState<EventReader<'static, 'static, ClientTickEvent<T>>>,
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

#[derive(Event)]
pub struct InsertComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> InsertComponentEvent<T, C> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

#[derive(Event)]
pub struct InsertBundleEvent<T: Send + Sync + 'static, B: ReplicateBundle> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<B>,
}

impl<T: Send + Sync + 'static, B: ReplicateBundle> InsertBundleEvent<T, B> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

#[derive(Event)]
pub struct UpdateComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub tick: Tick,
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> UpdateComponentEvent<T, C> {
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

#[derive(Event)]
pub struct RemoveComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    pub component: C,
}

impl<T: Send + Sync + 'static, C: Replicate> RemoveComponentEvent<T, C> {
    pub fn new(entity: Entity, component: C) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
            component,
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
