//! `EventReceiver<E>` — C.6-prep Sim-side inbound transport-event facade.
//!
//! # Why this exists
//!
//! Today cyberlith calls
//! [`crate::pipeline_actors::apply_receive_output_pipeline`]
//! against a bevy world to populate naia's per-event `Messages<X>` buffers,
//! then drains those buffers via `MessageReader<X>` from its own systems.
//! That works fine *when the consumer system runs in the same bevy world*
//! `apply_receive_output_pipeline` was called against. For the
//! Medium-symmetric C.6 architecture (cyberlith Sim is the only stage
//! holding user code), Sim wants to drain events without first having
//! `apply_receive_output_pipeline` run against Sim's world — that body
//! mutates the world (insert `ClientOwned`/`HostOwned` components,
//! despawn entities) and is byte-equivalent to what naia would do anyway,
//! but reasoning about the mutations from a Sim-system perspective is
//! noisy.
//!
//! `EventReceiver<E>` is a parking-lot-Mutex-backed shared facade:
//! every event type that `apply_receive_output_pipeline` fires into
//! `Messages<X>` is also fanned out into a per-type vector inside the
//! receiver. Sim systems drain by type via `drain_connect_events()` /
//! `drain_message_events()` / etc., and they never have to look at
//! naia's `Events<E>` flow directly.
//!
//! # Coexistence with `apply_receive_output_pipeline`
//!
//! Per the C.6-prep spec (§1.4): "the existing bevy `Messages<X>`
//! population is preserved during this mission so existing consumers
//! still work." Cyberlith pre-C.6 callers that drain Messages on Recv
//! continue to function. A consumer wanting the new facade installs a
//! `EventReceiver<E>` on Sim, and calls
//! [`EventReceiver::push_from_receive_output`] with each tick's
//! `ReceiveOutput<E>` (from whichever SubApp it has access to). The
//! Messages population is independent.
//!
//! # Why this is a facade, not an embedded worker
//!
//! The C.6-prep spec §1.4 proposes that naia internally pumps events
//! into the channels. Doing that cleanly requires naia to own the SubApp
//! identity / spawning, which the cyberlith
//! `_AGENTS/GAME_CELL_PIPELINING_TARGET.md` §3.1 explicitly assigns to
//! the consumer's orchestrator (Coord owns SubApps, never naia). The
//! facade splits the API surface cleanly: cyberlith owns the
//! SubApp-spawning + extract loop, naia provides the typed-receiver
//! resource and the `push_from_receive_output` adapter. The consumer's
//! one-line wiring inside its extract pass is the same `Resource::insert
//! + adapter call` that an embedded worker would have done internally.

use std::{hash::Hash, net::SocketAddr, sync::Arc};

use parking_lot::Mutex;

use naia_shared::{
    Channel, ChannelKind, DisconnectReason, Message, MessageContainer, MessageKind, Tick,
};

use crate::{events::Events, server::receive_output::ReceiveOutput, user::UserKey, EntityOwner};

use super::handles::CoordHandle;

// ────────────────────────────────────────────────────────────────────
// Per-event-type carrier structs (clone-friendly, Debug for tests).
// Fields are self-documenting and parallel the bevy `Messages<X>`
// payloads `apply_receive_output_pipeline` writes.
// ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RecvConnectEvent {
    pub user_key: UserKey,
}

#[derive(Clone, Debug)]
pub struct RecvDisconnectEvent {
    pub user_key: UserKey,
    pub address: SocketAddr,
    pub reason: DisconnectReason,
}

#[derive(Clone, Debug)]
pub struct RecvErrorEvent {
    /// Stringified `NaiaServerError` — the error variants carry
    /// non-Clone payloads in places, so we collapse to a String here
    /// for the channel-shared facade.
    pub error: String,
}

#[derive(Clone, Copy, Debug)]
pub struct RecvTickEvent(pub Tick);

#[derive(Clone, Debug)]
pub struct RecvSpawnEntityEvent<E> {
    pub user_key: UserKey,
    pub entity: E,
}

#[derive(Clone, Debug)]
pub struct RecvDespawnEntityEvent<E> {
    pub user_key: UserKey,
    pub entity: E,
}

#[derive(Clone, Debug)]
pub struct RecvPublishEntityEvent<E> {
    pub user_key: UserKey,
    pub entity: E,
}

#[derive(Clone, Debug)]
pub struct RecvUnpublishEntityEvent<E> {
    pub user_key: UserKey,
    pub entity: E,
}

// ────────────────────────────────────────────────────────────────────
// EventReceiver
// ────────────────────────────────────────────────────────────────────

/// Mutex-backed Sim-side inbound event receiver. Cloneable (Arc-internal).
/// Install once as a Sim Resource; clone if you need the consumer half
/// elsewhere.
///
/// All `drain_*` methods are non-blocking: they take the buffer for
/// their event type and return a `Vec` (empty when nothing has arrived).
pub struct EventReceiver<E: Copy + Eq + Hash + Send + Sync + 'static> {
    inner: Arc<EventReceiverInner<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Clone for EventReceiver<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct EventReceiverInner<E: Copy + Eq + Hash + Send + Sync + 'static> {
    connects: Mutex<Vec<RecvConnectEvent>>,
    disconnects: Mutex<Vec<RecvDisconnectEvent>>,
    errors: Mutex<Vec<RecvErrorEvent>>,
    ticks: Mutex<Vec<RecvTickEvent>>,
    spawns: Mutex<Vec<RecvSpawnEntityEvent<E>>>,
    despawns: Mutex<Vec<RecvDespawnEntityEvent<E>>>,
    publishes: Mutex<Vec<RecvPublishEntityEvent<E>>>,
    unpublishes: Mutex<Vec<RecvUnpublishEntityEvent<E>>>,
    /// Erased message storage — keyed by (ChannelKind, MessageKind).
    /// Drained by `drain_messages::<C, M>`.
    messages: Mutex<Vec<(ChannelKind, MessageKind, UserKey, MessageContainer)>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> EventReceiver<E> {
    /// Construct a fresh receiver.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventReceiverInner {
                connects: Mutex::new(Vec::new()),
                disconnects: Mutex::new(Vec::new()),
                errors: Mutex::new(Vec::new()),
                ticks: Mutex::new(Vec::new()),
                spawns: Mutex::new(Vec::new()),
                despawns: Mutex::new(Vec::new()),
                publishes: Mutex::new(Vec::new()),
                unpublishes: Mutex::new(Vec::new()),
                messages: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Fan out every event in `output` into the per-type vectors.
    /// Mirrors the body of `apply_receive_output_pipeline` but writes
    /// to the receiver instead of bevy `Messages<X>`.
    ///
    /// Consumes `output` (same semantics as
    /// `apply_receive_output_pipeline`); pre-existing Messages-based
    /// consumers should keep using `apply_receive_output_pipeline`
    /// directly and call this in parallel from the same SubApp.
    ///
    /// `sim_handle` is needed for the same resource-entity filter
    /// `apply_receive_output_pipeline` applies (Spawn / Despawn of
    /// resource entities are suppressed).
    pub fn push_from_receive_output(&self, sim_handle: &CoordHandle<E>, output: ReceiveOutput<E>) {
        let inner = &*self.inner;

        // Tick events
        if !output.pending_ticks.is_empty() {
            let mut guard = inner.ticks.lock();
            for tick in output.pending_ticks {
                guard.push(RecvTickEvent(tick));
            }
        }

        let mut events: Events<E> = Events::from(output.world_events);
        if events.is_empty() {
            return;
        }

        // Connect
        if events.has::<crate::ConnectEvent>() {
            let mut guard = inner.connects.lock();
            for user_key in events.read::<crate::ConnectEvent>() {
                guard.push(RecvConnectEvent { user_key });
            }
        }
        // Disconnect
        if events.has::<crate::DisconnectEvent>() {
            let mut guard = inner.disconnects.lock();
            for (user_key, address, reason) in events.read::<crate::DisconnectEvent>() {
                guard.push(RecvDisconnectEvent {
                    user_key,
                    address,
                    reason,
                });
            }
        }
        // Error
        if events.has::<crate::ErrorEvent>() {
            let mut guard = inner.errors.lock();
            for err in events.read::<crate::ErrorEvent>() {
                guard.push(RecvErrorEvent {
                    error: format!("{err:?}"),
                });
            }
        }
        // Spawn (resource-entity filter via sim_handle; client-owned only,
        // matching `apply_receive_output_pipeline`).
        if events.has::<crate::SpawnEntityEvent>() {
            let mut guard = inner.spawns.lock();
            for (_, entity) in events.read::<crate::SpawnEntityEvent>() {
                if sim_handle.is_resource_entity(&entity) {
                    continue;
                }
                if let EntityOwner::Client(user_key) = sim_handle.entity_owner(&entity) {
                    guard.push(RecvSpawnEntityEvent { user_key, entity });
                }
            }
        }
        // Despawn
        if events.has::<crate::DespawnEntityEvent>() {
            let mut guard = inner.despawns.lock();
            for (user_key, entity) in events.read::<crate::DespawnEntityEvent>() {
                if sim_handle.is_resource_entity(&entity) {
                    continue;
                }
                guard.push(RecvDespawnEntityEvent { user_key, entity });
            }
        }
        // Publish / Unpublish
        if events.has::<crate::PublishEntityEvent>() {
            let mut guard = inner.publishes.lock();
            for (user_key, entity) in events.read::<crate::PublishEntityEvent>() {
                guard.push(RecvPublishEntityEvent { user_key, entity });
            }
        }
        if events.has::<crate::UnpublishEntityEvent>() {
            let mut guard = inner.unpublishes.lock();
            for (user_key, entity) in events.read::<crate::UnpublishEntityEvent>() {
                guard.push(RecvUnpublishEntityEvent { user_key, entity });
            }
        }

        // Messages — stash erased; consumer types via `drain_messages::<C, M>`.
        if events.has_messages() {
            let mut guard = inner.messages.lock();
            let drained = events.take_messages();
            for (channel_kind, by_kind) in drained {
                for (message_kind, list) in by_kind {
                    for (user_key, container) in list {
                        guard.push((channel_kind, message_kind, user_key, container));
                    }
                }
            }
        }

        // Component / Bundle / Auth / Request event types are intentionally
        // NOT fanned out — they remain on `apply_receive_output_pipeline`'s
        // bevy `Messages<X>` path. Cyberlith Sim consumes typed component
        // events via bevy `MessageReader<InsertComponentEvent<C>>` etc.
    }

    /// Drain all queued connect events.
    pub fn drain_connect_events(&self) -> Vec<RecvConnectEvent> {
        std::mem::take(&mut *self.inner.connects.lock())
    }

    /// Drain all queued disconnect events.
    pub fn drain_disconnect_events(&self) -> Vec<RecvDisconnectEvent> {
        std::mem::take(&mut *self.inner.disconnects.lock())
    }

    /// Drain all queued error events.
    pub fn drain_error_events(&self) -> Vec<RecvErrorEvent> {
        std::mem::take(&mut *self.inner.errors.lock())
    }

    /// Drain all queued tick events.
    pub fn drain_tick_events(&self) -> Vec<RecvTickEvent> {
        std::mem::take(&mut *self.inner.ticks.lock())
    }

    /// Drain all queued client-owned entity-spawn events.
    pub fn drain_spawn_entity_events(&self) -> Vec<RecvSpawnEntityEvent<E>> {
        std::mem::take(&mut *self.inner.spawns.lock())
    }

    /// Drain all queued entity-despawn events.
    pub fn drain_despawn_entity_events(&self) -> Vec<RecvDespawnEntityEvent<E>> {
        std::mem::take(&mut *self.inner.despawns.lock())
    }

    /// Drain all queued entity-publish events.
    pub fn drain_publish_entity_events(&self) -> Vec<RecvPublishEntityEvent<E>> {
        std::mem::take(&mut *self.inner.publishes.lock())
    }

    /// Drain all queued entity-unpublish events.
    pub fn drain_unpublish_entity_events(&self) -> Vec<RecvUnpublishEntityEvent<E>> {
        std::mem::take(&mut *self.inner.unpublishes.lock())
    }

    // ────────────────────────────────────────────────────────────────
    // Typed push helpers — used by the combined helper
    // `apply_receive_output_pipeline_with_event_receiver` (in the bevy
    // server adapter) so it can drain a `ReceiveOutput<E>` once and
    // fan to both the bevy `Messages<X>` sinks AND this receiver
    // without double-consuming `WorldEvents<E>`.
    //
    // These are additive to `push_from_receive_output`; existing
    // single-sink callers stay on that method.
    // ────────────────────────────────────────────────────────────────

    /// Push a single tick into the tick buffer.
    pub fn push_tick(&self, tick: Tick) {
        self.inner.ticks.lock().push(RecvTickEvent(tick));
    }

    /// Push a single connect event into the connect buffer.
    pub fn push_connect(&self, user_key: UserKey) {
        self.inner
            .connects
            .lock()
            .push(RecvConnectEvent { user_key });
    }

    /// Push a single disconnect event into the disconnect buffer.
    pub fn push_disconnect(
        &self,
        user_key: UserKey,
        address: SocketAddr,
        reason: DisconnectReason,
    ) {
        self.inner.disconnects.lock().push(RecvDisconnectEvent {
            user_key,
            address,
            reason,
        });
    }

    /// Push a single error event (already-formatted message) into the
    /// error buffer. Mirrors `push_from_receive_output`'s
    /// `format!("{err:?}")` collapse since `NaiaServerError` variants
    /// carry non-Clone payloads.
    pub fn push_error(&self, error: String) {
        self.inner.errors.lock().push(RecvErrorEvent { error });
    }

    /// Push a client-owned spawn event into the spawn buffer.
    /// Caller is responsible for the resource-entity + client-owner
    /// filter (mirroring `push_from_receive_output`).
    pub fn push_spawn_client_owned(&self, user_key: UserKey, entity: E) {
        self.inner
            .spawns
            .lock()
            .push(RecvSpawnEntityEvent { user_key, entity });
    }

    /// Push a despawn event into the despawn buffer.
    /// Caller is responsible for the resource-entity filter.
    pub fn push_despawn(&self, user_key: UserKey, entity: E) {
        self.inner
            .despawns
            .lock()
            .push(RecvDespawnEntityEvent { user_key, entity });
    }

    /// Push a publish event into the publish buffer.
    pub fn push_publish(&self, user_key: UserKey, entity: E) {
        self.inner
            .publishes
            .lock()
            .push(RecvPublishEntityEvent { user_key, entity });
    }

    /// Push an unpublish event into the unpublish buffer.
    pub fn push_unpublish(&self, user_key: UserKey, entity: E) {
        self.inner
            .unpublishes
            .lock()
            .push(RecvUnpublishEntityEvent { user_key, entity });
    }

    /// Push the entries of a (channel_kind, message_kind) entry into
    /// the erased messages buffer. `MessageContainer` is Arc-internal
    /// so cloning is cheap.
    pub fn push_message(
        &self,
        channel_kind: ChannelKind,
        message_kind: MessageKind,
        user_key: UserKey,
        container: MessageContainer,
    ) {
        self.inner
            .messages
            .lock()
            .push((channel_kind, message_kind, user_key, container));
    }

    /// Drain all messages of type `M` received on channel `C`. Other
    /// channel/message combinations are left in the buffer for their
    /// own type-specific drainers.
    pub fn drain_messages<C: Channel, M: Message>(&self) -> Vec<(UserKey, M)> {
        let want_channel = ChannelKind::of::<C>();
        let want_message = MessageKind::of::<M>();
        let mut out = Vec::new();
        let mut guard = self.inner.messages.lock();
        let mut keep = Vec::with_capacity(guard.len());
        for (ck, mk, user_key, container) in guard.drain(..) {
            if ck == want_channel && mk == want_message {
                match container.to_boxed_any().downcast::<M>() {
                    Ok(boxed) => {
                        out.push((user_key, *boxed));
                        continue;
                    }
                    Err(_boxed_any) => {
                        // Type mismatch — this should be impossible given
                        // the kind keys match. Drop the message rather
                        // than panicking; this preserves the existing
                        // `Events<E>` semantics.
                        continue;
                    }
                }
            }
            keep.push((ck, mk, user_key, container));
        }
        *guard = keep;
        out
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Default for EventReceiver<E> {
    fn default() -> Self {
        Self::new()
    }
}
