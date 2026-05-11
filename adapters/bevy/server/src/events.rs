use std::{any::Any, collections::HashMap, hash::Hash, marker::PhantomData, net::SocketAddr};

use bevy_ecs::{
    entity::Entity,
    message::{MessageCursor, Messages},
    prelude::Resource,
    system::{Res, SystemState},
};

use naia_bevy_shared::{
    Channel, ChannelKind, Message, MessageContainer, MessageKind, Replicate, ReplicateBundle,
    Request, ResponseSendKey, Tick,
};

use naia_server::{shared::GlobalResponseId, Events, NaiaServerError, UserKey};
use naia_server::DisconnectReason;

// ConnectEvent
#[derive(bevy_ecs::message::Message)]
pub struct ConnectEvent(pub UserKey);

// DisconnectEvent
#[derive(bevy_ecs::message::Message)]
pub struct DisconnectEvent(pub UserKey, pub SocketAddr, pub DisconnectReason);

// ErrorEvent
#[derive(bevy_ecs::message::Message)]
pub struct ErrorEvent(pub NaiaServerError);

// TickEventReader
#[derive(Resource)]
pub(crate) struct CachedTickEventsState {
    pub(crate) event_state: SystemState<(
        Res<'static, Messages<TickEvent>>,
        bevy_ecs::system::Local<'static, MessageCursor<TickEvent>>,
    )>,
}

// TickEvent
#[derive(bevy_ecs::message::Message)]
pub struct TickEvent(pub Tick);

// AuthEvents
#[derive(bevy_ecs::message::Message)]
pub struct AuthEvents {
    inner: HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>,
}

impl<E: Hash + Copy + Eq + Sync + Send> From<&mut Events<E>> for AuthEvents {
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
#[derive(bevy_ecs::message::Message)]
pub struct MessageEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>,
}

impl<E: Hash + Copy + Eq + Sync + Send> From<&mut Events<E>> for MessageEvents {
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

// RequestEvents
#[derive(bevy_ecs::message::Message)]
pub struct RequestEvents {
    inner: HashMap<
        ChannelKind,
        HashMap<MessageKind, Vec<(UserKey, GlobalResponseId, MessageContainer)>>,
    >,
}

impl<E: Hash + Copy + Eq + Sync + Send> From<&mut Events<E>> for RequestEvents {
    fn from(events: &mut Events<E>) -> Self {
        Self {
            inner: events.take_requests(),
        }
    }
}

impl RequestEvents {
    pub fn read<C: Channel, Q: Request>(&self) -> Vec<(UserKey, ResponseSendKey<Q::Response>, Q)> {
        let channel_kind = ChannelKind::of::<C>();
        let Some(request_map) = self.inner.get(&channel_kind) else {
            return Vec::new();
        };
        let message_kind = MessageKind::of::<Q>();
        let Some(requests) = request_map.get(&message_kind) else {
            return Vec::new();
        };

        let mut output_list: Vec<(UserKey, ResponseSendKey<Q::Response>, Q)> = Vec::new();

        for (user_key, global_response_id, request) in requests {
            let message: Q =
                Box::<dyn Any + 'static>::downcast::<Q>(request.clone().to_boxed_any())
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .unwrap();

            let response_send_key = ResponseSendKey::new(*global_response_id);

            output_list.push((*user_key, response_send_key, message));
        }

        output_list
    }
}

// SpawnEntityEvent
#[derive(bevy_ecs::message::Message)]
pub struct SpawnEntityEvent(pub UserKey, pub Entity);

// DespawnEntityEvent
#[derive(bevy_ecs::message::Message)]
pub struct DespawnEntityEvent(pub UserKey, pub Entity);

// PublishEntityEvent
#[derive(bevy_ecs::message::Message)]
pub struct PublishEntityEvent(pub UserKey, pub Entity);

// UnpublishEntityEvent
#[derive(bevy_ecs::message::Message)]
pub struct UnpublishEntityEvent(pub UserKey, pub Entity);

#[derive(bevy_ecs::message::Message)]
pub struct InsertComponentEvent<C: Replicate> {
    pub user_key: UserKey,
    pub entity: Entity,
    phantom_c: PhantomData<C>,
}

impl<C: Replicate> InsertComponentEvent<C> {
    pub fn new(user_key: UserKey, entity: Entity) -> Self {
        Self {
            user_key,
            entity,
            phantom_c: PhantomData,
        }
    }
}

#[derive(bevy_ecs::message::Message)]
pub struct InsertBundleEvent<B: ReplicateBundle> {
    pub user_key: UserKey,
    pub entity: Entity,
    phantom_c: PhantomData<B>,
}

impl<B: ReplicateBundle> InsertBundleEvent<B> {
    pub fn new(user_key: UserKey, entity: Entity) -> Self {
        Self {
            user_key,
            entity,
            phantom_c: PhantomData,
        }
    }
}

#[derive(bevy_ecs::message::Message)]
pub struct UpdateComponentEvent<C: Replicate> {
    pub user_key: UserKey,
    pub entity: Entity,
    phantom_c: PhantomData<C>,
}

impl<C: Replicate> UpdateComponentEvent<C> {
    pub fn new(user_key: UserKey, entity: Entity) -> Self {
        Self {
            user_key,
            entity,
            phantom_c: PhantomData,
        }
    }
}

#[derive(bevy_ecs::message::Message)]
pub struct RemoveComponentEvent<C: Replicate> {
    pub user_key: UserKey,
    pub entity: Entity,
    pub component: C,
}

impl<C: Replicate> RemoveComponentEvent<C> {
    pub fn new(user_key: UserKey, entity: Entity, component: C) -> Self {
        Self {
            user_key,
            entity,
            component,
        }
    }
}

// =====================================================================
// Replicated Resource Events (D13 — user-facing, no entity field)
// =====================================================================
//
// These mirror the Component event types but strip the `entity` field:
// users see ZERO entity-level semantics for resources (D13). The bevy
// adapter event-emission filter routes resource entities out of
// SpawnEntityEvent / DespawnEntityEvent / component events; the
// equivalent resource events fire instead.

/// Fires when a Replicated Resource of type `R` becomes available on
/// the server (newly inserted, OR a connecting client first observes
/// it — see D20 for late-join semantics applied to the client side).
#[derive(bevy_ecs::message::Message)]
pub struct InsertResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> InsertResourceEvent<R> {
    pub fn new(user_key: UserKey) -> Self {
        Self {
            user_key,
            phantom_r: PhantomData,
        }
    }
}

/// Fires whenever a Replicated Resource of type `R` is updated by the
/// authority holder (server-authoritative, or client when client holds
/// authority on a delegated resource).
#[derive(bevy_ecs::message::Message)]
pub struct UpdateResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> UpdateResourceEvent<R> {
    pub fn new(user_key: UserKey) -> Self {
        Self {
            user_key,
            phantom_r: PhantomData,
        }
    }
}

/// Fires when a Replicated Resource of type `R` is removed.
#[derive(bevy_ecs::message::Message)]
pub struct RemoveResourceEvent<R: Replicate> {
    pub user_key: UserKey,
    pub resource: R,
}

impl<R: Replicate> RemoveResourceEvent<R> {
    pub fn new(user_key: UserKey, resource: R) -> Self {
        Self {
            user_key,
            resource,
        }
    }
}
