use std::{any::Any, collections::HashMap, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    message::{MessageCursor, Messages},
    resource::Resource,
    system::SystemState,
};

use naia_client::{shared::GlobalResponseId, NaiaClientError, Events};
use naia_client::DisconnectReason;

use naia_bevy_shared::{
    Channel, ChannelKind, Message, MessageContainer, MessageKind, ReplicateBundle, Request,
    ResponseSendKey, Tick,
};

use crate::Replicate;

// ConnectEvent
#[derive(bevy_ecs::message::Message)]
pub struct ConnectEvent<T> {
    phantom_t: PhantomData<T>,
}

impl<T> Default for ConnectEvent<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ConnectEvent<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// DisconnectEvent
#[derive(bevy_ecs::message::Message)]
pub struct DisconnectEvent<T> {
    pub reason: DisconnectReason,
    phantom_t: PhantomData<T>,
}

impl<T> Default for DisconnectEvent<T> {
    fn default() -> Self {
        Self::new(DisconnectReason::ClientDisconnected)
    }
}

impl<T> DisconnectEvent<T> {
    pub fn new(reason: DisconnectReason) -> Self {
        Self {
            reason,
            phantom_t: PhantomData,
        }
    }
}

// RejectEvent
#[derive(bevy_ecs::message::Message)]
pub struct RejectEvent<T> {
    phantom_t: PhantomData<T>,
}

impl<T> Default for RejectEvent<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> RejectEvent<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// ErrorEvent
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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

// RequestEvents
#[derive(bevy_ecs::message::Message)]
pub struct RequestEvents<T> {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>>,
    phantom_t: PhantomData<T>,
}

impl<T> From<&mut Events<Entity>> for RequestEvents<T> {
    fn from(events: &mut Events<Entity>) -> Self {
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
    #[allow(clippy::type_complexity)]
    pub(crate) event_state: SystemState<(
        bevy_ecs::system::Res<'static, Messages<ClientTickEvent<T>>>,
        bevy_ecs::system::Local<'static, MessageCursor<ClientTickEvent<T>>>,
    )>,
}

// ClientTickEvent
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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

#[derive(bevy_ecs::message::Message)]
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

#[derive(bevy_ecs::message::Message)]
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

#[derive(bevy_ecs::message::Message)]
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

#[derive(bevy_ecs::message::Message)]
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

// =====================================================================
// Replicated Resource Events (D13 — user-facing, no entity field)
// =====================================================================
//
// Mirror of the server-side resource events. Per D13/D17, these are
// the user-visible event surface for Replicated Resources on the
// client; users never see SpawnEntityEvent / InsertComponentEvent for
// resource entities.

/// Fires when a Replicated Resource of type `R` first becomes visible
/// to this client. Per D20, late-join is indistinguishable from
/// fresh-spawn at the event level — this fires whether `R` was just
/// inserted on the server OR was inserted long ago and the client just
/// connected.
#[derive(bevy_ecs::message::Message)]
pub struct InsertResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    phantom_t: PhantomData<T>,
    phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> InsertResourceEvent<T, R> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
            phantom_r: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, R: Replicate> Default for InsertResourceEvent<T, R> {
    fn default() -> Self {
        Self::new()
    }
}

/// Fires whenever a Replicated Resource of type `R` is updated by the
/// authority holder.
#[derive(bevy_ecs::message::Message)]
pub struct UpdateResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    pub tick: Tick,
    phantom_t: PhantomData<T>,
    phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> UpdateResourceEvent<T, R> {
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            phantom_t: PhantomData,
            phantom_r: PhantomData,
        }
    }
}

/// Fires when a Replicated Resource of type `R` is removed (server-
/// authoritative removal, OR despawn from this client's scope).
#[derive(bevy_ecs::message::Message)]
pub struct RemoveResourceEvent<T: Send + Sync + 'static, R: Replicate> {
    phantom_t: PhantomData<T>,
    pub resource: R,
}

impl<T: Send + Sync + 'static, R: Replicate> RemoveResourceEvent<T, R> {
    pub fn new(resource: R) -> Self {
        Self {
            phantom_t: PhantomData,
            resource,
        }
    }
}

// PublishEntityEvent
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
#[derive(bevy_ecs::message::Message)]
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
