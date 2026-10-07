use std::{any::Any, collections::HashMap, marker::PhantomData, net::SocketAddr};

use bevy_ecs::{
    entity::Entity,
    message::{MessageCursor, Messages},
    resource::Resource,
    system::SystemState,
};

use naia_client::DisconnectReason;
use naia_client::{
    shared::GlobalResponseId, Events, LifecycleKind, LifecycleOrder, NaiaClientError, RejectReason,
};

use naia_bevy_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, ReplicateBundle,
    Request, ResponseSendKey, Tick,
};

use crate::Replicate;

// ConnectEvent
/// Fires once the connection to the server is established.
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
    /// Creates a new `ConnectEvent`.
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}

// DisconnectEvent
/// Fires when the connection to the server is lost.
///
/// `message` carries what the server enclosed via
/// `Server::disconnect_user_with` (naia-lib/naia#10); it is `None` for a
/// timeout, a client-initiated disconnect, or a plain kick. Downcast it with
/// `container.to_boxed_any().downcast::<YourMessage>()`.
#[derive(bevy_ecs::message::Message)]
pub struct DisconnectEvent<T> {
    /// Why the connection was lost.
    pub reason: DisconnectReason,
    /// The server-enclosed message, if any; see the struct docs.
    pub message: Option<MessageContainer>,
    phantom_t: PhantomData<T>,
}

impl<T> Default for DisconnectEvent<T> {
    fn default() -> Self {
        Self::new(DisconnectReason::ClientDisconnected, None)
    }
}

impl<T> DisconnectEvent<T> {
    /// Creates a new `DisconnectEvent` with the given reason and optional
    /// server message.
    pub fn new(reason: DisconnectReason, message: Option<MessageContainer>) -> Self {
        Self {
            reason,
            message,
            phantom_t: PhantomData,
        }
    }
}

// RejectEvent
/// Fires when the server refused the connection.
///
/// `address` is the server address when one is known: a pre-auth rejection
/// happens before the data address is learned, so it is `None` there, while a
/// post-address in-band rejection carries `Some`. No sentinel is manufactured.
///
/// `reason` mirrors the underlying client refusal exactly: `ProtocolMismatch`
/// for a pre-protocol refusal, `Auth` for an application rejection. It is a
/// plain field (not folded into `message`) so handlers can match on it without
/// decoding anything.
///
/// `message` carries the reason the server sent with
/// `reject_connection_with`, if any (naia-lib/naia#133). Downcast it with
/// `container.to_boxed_any().downcast::<MyRejectReason>()`.
#[derive(bevy_ecs::message::Message)]
pub struct RejectEvent<T> {
    /// The server address, when known; see the struct docs.
    pub address: Option<SocketAddr>,
    /// The underlying refusal reason, mirrored verbatim.
    pub reason: RejectReason,
    /// The reason the server sent with `reject_connection_with`, if any.
    pub message: Option<MessageContainer>,
    phantom_t: PhantomData<T>,
}

impl<T> Default for RejectEvent<T> {
    fn default() -> Self {
        Self::new(None, RejectReason::Auth, None)
    }
}

impl<T> RejectEvent<T> {
    /// Creates a new `RejectEvent` with the given address, reason, and
    /// optional server message.
    pub fn new(
        address: Option<SocketAddr>,
        reason: RejectReason,
        message: Option<MessageContainer>,
    ) -> Self {
        Self {
            address,
            reason,
            message,
            phantom_t: PhantomData,
        }
    }
}

// ErrorEvent
/// Fires when the underlying naia client reports an error.
#[derive(bevy_ecs::message::Message)]
pub struct ErrorEvent<T> {
    /// The underlying client error.
    pub err: NaiaClientError,
    phantom_t: PhantomData<T>,
}

impl<T> ErrorEvent<T> {
    /// Creates a new `ErrorEvent` wrapping `err`.
    pub fn new(err: NaiaClientError) -> Self {
        Self {
            err,
            phantom_t: PhantomData,
        }
    }
}

// MessageEvents
/// Carries every message received this frame, grouped by channel and
/// message type; read with [`MessageEvents::read`].
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
    /// Returns every message of type `M` received on channel `C` this
    /// frame, downcast from their boxed form. Empty if none arrived.
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
/// Carries every request received this frame, grouped by channel and
/// request type; read with [`RequestEvents::read`].
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
    /// Returns every request of type `Q` received on channel `C` this
    /// frame, each paired with the key to send its response. Empty if none
    /// arrived.
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
/// Fires once per client tick, carrying the tick that just elapsed.
#[derive(bevy_ecs::message::Message)]
pub struct ClientTickEvent<T> {
    /// The client tick that just elapsed.
    pub tick: Tick,
    phantom_t: PhantomData<T>,
}

impl<T> ClientTickEvent<T> {
    /// Creates a new `ClientTickEvent` for `tick`.
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            phantom_t: PhantomData,
        }
    }
}

// ServerTickEvent
/// Fires once per newly-received server tick, carrying that tick.
#[derive(bevy_ecs::message::Message)]
pub struct ServerTickEvent<T> {
    /// The server tick that was just received.
    pub tick: Tick,
    phantom_t: PhantomData<T>,
}

impl<T> ServerTickEvent<T> {
    /// Creates a new `ServerTickEvent` for `tick`.
    pub fn new(tick: Tick) -> Self {
        Self {
            tick,
            phantom_t: PhantomData,
        }
    }
}

// SpawnEntityEvent
/// Fires when the server spawns an entity into this client's scope.
#[derive(bevy_ecs::message::Message)]
pub struct SpawnEntityEvent<T> {
    /// The tick at which the entity was spawned.
    pub tick: Tick,
    /// The spawned entity.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> SpawnEntityEvent<T> {
    /// Creates a new `SpawnEntityEvent` for `entity` at `tick`.
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
        }
    }
}

// DespawnEntityEvent
/// Fires when the server despawns an entity, or it leaves this client's
/// scope.
#[derive(bevy_ecs::message::Message)]
pub struct DespawnEntityEvent<T> {
    /// The tick at which the entity was despawned.
    pub tick: Tick,
    /// The despawned entity.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> DespawnEntityEvent<T> {
    /// Creates a new `DespawnEntityEvent` for `entity` at `tick`.
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
        }
    }
}

/// Fires when a component of type `C` is inserted (replicated) onto
/// `entity`. Not fired for a component registered as part of a resource via
/// `add_resource_events` — see [`InsertResourceEvent`].
#[derive(bevy_ecs::message::Message)]
pub struct InsertComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    /// The tick at which the component was inserted.
    pub tick: Tick,
    /// The entity the component was inserted on.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> InsertComponentEvent<T, C> {
    /// Creates a new `InsertComponentEvent` for `entity` at `tick`.
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

/// Fires once all components of registered bundle `B` are present on
/// `entity`. Tickless — see the bundle registry's dedup-by-entity tracking.
#[derive(bevy_ecs::message::Message)]
pub struct InsertBundleEvent<T: Send + Sync + 'static, B: ReplicateBundle> {
    /// The entity on which the bundle's components are all now present.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<B>,
}

impl<T: Send + Sync + 'static, B: ReplicateBundle> InsertBundleEvent<T, B> {
    /// Creates a new `InsertBundleEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

/// Fires when a component of type `C` on `entity` is updated by the server.
#[derive(bevy_ecs::message::Message)]
pub struct UpdateComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    /// The tick at which the update was applied.
    pub tick: Tick,
    /// The entity whose component was updated.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    phantom_c: PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> UpdateComponentEvent<T, C> {
    /// Creates a new `UpdateComponentEvent` for `entity` at `tick`.
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
            phantom_c: PhantomData,
        }
    }
}

/// Fires when a component of type `C` is removed from `entity`, carrying
/// the removed value.
#[derive(bevy_ecs::message::Message)]
pub struct RemoveComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    /// The tick at which the component was removed.
    pub tick: Tick,
    /// The entity the component was removed from.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
    /// The removed component's last known value.
    pub component: C,
}

impl<T: Send + Sync + 'static, C: Replicate> RemoveComponentEvent<T, C> {
    /// Creates a new `RemoveComponentEvent` for `entity` at `tick`, carrying
    /// the removed `component`.
    pub fn new(tick: Tick, entity: Entity, component: C) -> Self {
        Self {
            tick,
            entity,
            phantom_t: PhantomData,
            component,
        }
    }
}

// LifecycleEvent
/// One entry of the tick-tagged, cross-kind lifecycle stream.
///
/// GUARANTEE: messages in this stream arrive in the server's application
/// order — the order the client applied the transitions. Entries are
/// ordered by arrival, so each tick's spawn/insert/remove/despawn chain
/// reads in send order, and a later tick sorts after an earlier one. A
/// consumer that needs the order ACROSS kinds (an insert followed by a
/// remove in the same frame) reads this stream; the per-kind wrappers
/// above group by kind and cannot express that interleave.
///
/// `component_kind` is `Some` for inserts and removes, `None` for spawns
/// and despawns. Payloads (e.g. the removed component value) stay on the
/// typed wrappers; this stream carries order, tick, and identity.
#[derive(bevy_ecs::message::Message)]
pub struct LifecycleEvent<T> {
    /// The tick at which the transition happened.
    pub tick: Tick,
    /// Which lifecycle transition this entry is.
    pub kind: LifecycleKind,
    /// The entity the transition applies to.
    pub entity: Entity,
    /// The component kind involved, for inserts and removes; `None` for
    /// spawns and despawns.
    pub component_kind: Option<ComponentKind>,
    phantom_t: PhantomData<T>,
}

impl<T> LifecycleEvent<T> {
    /// Creates a new `LifecycleEvent` entry.
    pub fn new(
        tick: Tick,
        kind: LifecycleKind,
        entity: Entity,
        component_kind: Option<ComponentKind>,
    ) -> Self {
        Self {
            tick,
            kind,
            entity,
            component_kind,
            phantom_t: PhantomData,
        }
    }
}

/// Translates one drained low-level lifecycle stream into Bevy messages,
/// preserving the server's application order entry by entry.
pub(crate) fn lifecycle_messages<T>(
    entries: Vec<LifecycleOrder<Entity>>,
) -> Vec<LifecycleEvent<T>> {
    entries
        .into_iter()
        .map(|entry| {
            LifecycleEvent::<T>::new(entry.tick, entry.kind, entry.entity, entry.component_kind)
        })
        .collect()
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
    /// Creates a new `InsertResourceEvent`.
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
    /// The tick at which the resource was updated.
    pub tick: Tick,
    phantom_t: PhantomData<T>,
    phantom_r: PhantomData<R>,
}

impl<T: Send + Sync + 'static, R: Replicate> UpdateResourceEvent<T, R> {
    /// Creates a new `UpdateResourceEvent` for `tick`.
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
    /// The removed resource's last known value.
    pub resource: R,
}

impl<T: Send + Sync + 'static, R: Replicate> RemoveResourceEvent<T, R> {
    /// Creates a new `RemoveResourceEvent` carrying the removed `resource`.
    pub fn new(resource: R) -> Self {
        Self {
            phantom_t: PhantomData,
            resource,
        }
    }
}

// PublishEntityEvent
/// Fires when a client-authoritative entity becomes visible to the server
/// (published).
#[derive(bevy_ecs::message::Message)]
pub struct PublishEntityEvent<T> {
    /// The published entity.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> PublishEntityEvent<T> {
    /// Creates a new `PublishEntityEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// UnpublishEntityEvent
/// Fires when a previously-published client-authoritative entity is
/// unpublished (withdrawn from the server's scope).
#[derive(bevy_ecs::message::Message)]
pub struct UnpublishEntityEvent<T> {
    /// The unpublished entity.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> UnpublishEntityEvent<T> {
    /// Creates a new `UnpublishEntityEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthGrantedEvent
/// Fires when the server grants this client authority over `entity`
/// (requested via `CommandsExt::request_authority`).
#[derive(bevy_ecs::message::Message)]
pub struct EntityAuthGrantedEvent<T> {
    /// The entity authority was granted over.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthGrantedEvent<T> {
    /// Creates a new `EntityAuthGrantedEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthDeniedEvent
/// Fires when the server denies this client's authority request over
/// `entity`.
#[derive(bevy_ecs::message::Message)]
pub struct EntityAuthDeniedEvent<T> {
    /// The entity whose authority request was denied.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthDeniedEvent<T> {
    /// Creates a new `EntityAuthDeniedEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

// EntityAuthResetEvent
/// Fires when this client's previously-granted authority over `entity` is
/// reset back to the server.
#[derive(bevy_ecs::message::Message)]
pub struct EntityAuthResetEvent<T> {
    /// The entity whose authority was reset.
    pub entity: Entity,
    phantom_t: PhantomData<T>,
}

impl<T> EntityAuthResetEvent<T> {
    /// Creates a new `EntityAuthResetEvent` for `entity`.
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constructor carries the refusal verbatim. Exact `ProtocolMismatch`
    /// and `Auth` propagation is pinned here, so a dropped or substituted
    /// reason goes red.
    #[test]
    fn reject_event_constructor_propagates_the_exact_reason() {
        let addr: SocketAddr = "127.0.0.1:14191".parse().unwrap();

        let mismatch = RejectEvent::<()>::new(Some(addr), RejectReason::ProtocolMismatch, None);
        assert_eq!(mismatch.address, Some(addr));
        assert_eq!(mismatch.reason, RejectReason::ProtocolMismatch);
        assert!(mismatch.message.is_none());

        let auth = RejectEvent::<()>::new(None, RejectReason::Auth, None);
        assert_eq!(auth.address, None);
        assert_eq!(auth.reason, RejectReason::Auth);
        assert!(auth.message.is_none());

        assert_ne!(
            mismatch.reason, auth.reason,
            "the two refusal kinds must stay distinguishable at the Bevy boundary",
        );
    }

    /// The spawn wrapper must carry the tick it applies to: the observer
    /// cannot place a spawn on the rollback timeline without it.
    #[test]
    fn spawn_entity_event_constructor_carries_tick() {
        let entity = Entity::from_bits(7);
        let event = SpawnEntityEvent::<()>::new(20, entity);
        assert_eq!(event.tick, 20);
        assert_eq!(event.entity, entity);
    }

    /// Same for despawns: a tickless despawn cannot retire the entity's
    /// confirmed history.
    #[test]
    fn despawn_entity_event_constructor_carries_tick() {
        let entity = Entity::from_bits(7);
        let event = DespawnEntityEvent::<()>::new(21, entity);
        assert_eq!(event.tick, 21);
        assert_eq!(event.entity, entity);
    }

    /// The component wrappers are generic over `C: Replicate`, which has no
    /// test impl in this crate, so the tick is pinned on the source instead:
    /// each struct must declare it and take it in its constructor. A
    /// wrapper that silently dropped the tick would compile downstream and
    /// misorder the observer, so the text is the contract.
    #[test]
    fn component_event_wrappers_declare_tick() {
        const THIS_FILE: &str = include_str!("events.rs");

        let insert = section(
            THIS_FILE,
            "pub struct InsertComponentEvent",
            "pub struct InsertBundleEvent",
        );
        assert!(
            insert.contains("pub tick: Tick,"),
            "InsertComponentEvent must carry the tick",
        );
        assert!(
            insert.contains("pub fn new(tick: Tick, entity: Entity)"),
            "InsertComponentEvent::new must take the tick",
        );

        let remove = section(
            THIS_FILE,
            "pub struct RemoveComponentEvent",
            "// Replicated Resource Events",
        );
        assert!(
            remove.contains("pub tick: Tick,"),
            "RemoveComponentEvent must carry the tick",
        );
    }

    /// The ordered lifecycle stream must preserve the server's application
    /// order across kinds: spawn, insert, remove, despawn inside one tick
    /// come out in that order, and a later tick sorts after.
    #[test]
    fn lifecycle_messages_preserve_server_order_across_kinds() {
        let entity = Entity::from_bits(7);
        let other = Entity::from_bits(9);
        let entries = vec![
            LifecycleOrder {
                tick: 20,
                kind: LifecycleKind::Spawn,
                entity,
                component_kind: None,
            },
            LifecycleOrder {
                tick: 20,
                kind: LifecycleKind::Insert,
                entity,
                component_kind: None,
            },
            LifecycleOrder {
                tick: 20,
                kind: LifecycleKind::Remove,
                entity,
                component_kind: None,
            },
            LifecycleOrder {
                tick: 21,
                kind: LifecycleKind::Despawn,
                entity,
                component_kind: None,
            },
            LifecycleOrder {
                tick: 22,
                kind: LifecycleKind::Spawn,
                entity: other,
                component_kind: None,
            },
        ];

        let messages = lifecycle_messages::<()>(entries);

        assert_eq!(
            messages
                .iter()
                .map(|message| (message.tick, message.kind, message.entity))
                .collect::<Vec<_>>(),
            vec![
                (20, LifecycleKind::Spawn, entity),
                (20, LifecycleKind::Insert, entity),
                (20, LifecycleKind::Remove, entity),
                (21, LifecycleKind::Despawn, entity),
                (22, LifecycleKind::Spawn, other),
            ],
        );
    }

    fn section<'a>(file: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
        let start = file
            .find(start_marker)
            .expect("lifecycle wrappers must keep their shape");
        let body = &file[start..];
        let end = body
            .find(end_marker)
            .expect("lifecycle wrappers must keep their shape");
        &body[..end]
    }

    /// A downstream Bevy consumer matches both refusal kinds through the
    /// supported root export. This pins the `naia_client::RejectReason`
    /// re-export: if it ever narrows, moves, or goes private, every
    /// downstream exhaustive `match` breaks here first, at the public API.
    ///
    /// The `use super::*` above names the reason only through the crate
    /// root (`naia_client::RejectReason`, as imported at the top of this
    /// file). Reaching into `naia_client::handshake` or any other private
    /// module instead would fail to compile -- there is no other path, by
    /// construction, so this test cannot pass on a private import.
    #[test]
    fn downstream_consumers_match_both_reasons_through_the_public_export() {
        fn describe(reason: RejectReason) -> &'static str {
            match reason {
                RejectReason::ProtocolMismatch => "mismatch",
                RejectReason::Auth => "auth",
            }
        }

        assert_eq!(describe(RejectReason::ProtocolMismatch), "mismatch");
        assert_eq!(describe(RejectReason::Auth), "auth");
    }
}
