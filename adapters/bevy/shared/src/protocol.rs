use std::{sync::Arc, time::Duration};

use bevy_app::App;
use bevy_ecs::component::{Component, Mutable};

use naia_shared::{
    Channel, ChannelDirection, ChannelMode, ComponentKind, CompressionConfig,
    LinkConditionerConfig, Message, Protocol as InnerProtocol, Replicate, Request,
};

use crate::{snapshot_reader_registry::SnapshotReaderRegistry, ProtocolPlugin, WorldData};

/// A type-erased closure that registers Bevy events/systems on the `App`.
/// Named because the bare `Arc<dyn Fn(&mut App) + Send + Sync>` appears in
/// four places and reads as noise at each of them.
pub type EventInstaller = Arc<dyn Fn(&mut App) + Send + Sync>;

/// Bevy-facing builder for the wire protocol: wraps `naia_shared::Protocol`
/// and additionally tracks `WorldData`, the snapshot-reader registry, and
/// the deferred client/server event-installer closures that `add_component`
/// extension methods register.
#[derive(Clone)]
pub struct Protocol {
    inner: InnerProtocol,
    world_data: Option<WorldData>,
    snapshot_readers: SnapshotReaderRegistry,
    client_event_installers: Vec<EventInstaller>,
    server_event_installers: Vec<EventInstaller>,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            inner: InnerProtocol::default(),
            world_data: Some(WorldData::new()),
            snapshot_readers: SnapshotReaderRegistry::new(),
            client_event_installers: Vec::new(),
            server_event_installers: Vec::new(),
        }
    }
}

impl Protocol {
    /// Creates a new `Protocol` builder (alias for [`Protocol::default`]).
    pub fn builder() -> Self {
        Self::default()
    }

    /// Takes the `WorldData` out of this `Protocol`. Panics if called more
    /// than once.
    pub fn take_world_data(&mut self) -> WorldData {
        self.world_data.take().expect("should only call this once")
    }

    /// Applies a [`ProtocolPlugin`], letting it register its own channels,
    /// messages, and components on this `Protocol`.
    pub fn add_plugin<P: ProtocolPlugin>(&mut self, plugin: P) -> &mut Self {
        self.check_lock();
        plugin.build(self);
        self
    }

    /// Configures simulated network conditions (latency, jitter, loss) for
    /// local testing.
    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.inner.link_condition(config);
        self
    }

    /// Allows clients to spawn and have authority over their own entities.
    pub fn enable_client_authoritative_entities(&mut self) -> &mut Self {
        self.inner.enable_client_authoritative_entities();
        self
    }

    /// Sets the HTTP path the server exposes for WebRTC session negotiation.
    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.inner.rtc_endpoint(path);
        self
    }

    /// Returns the configured WebRTC session-negotiation path.
    pub fn get_rtc_endpoint(&self) -> String {
        self.inner.get_rtc_endpoint()
    }

    /// Sets how often the server advances a network tick.
    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.inner.tick_interval(duration);
        self
    }

    /// Configures wire compression.
    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.inner.compression(config);
        self
    }

    /// Registers naia's built-in default channels.
    pub fn add_default_channels(&mut self) -> &mut Self {
        self.inner.add_default_channels();
        self
    }

    /// Registers channel `C` with the given direction and delivery mode.
    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        self.inner.add_channel::<C>(direction, mode);
        self
    }

    /// Registers message type `M` for wire (de)serialization.
    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.inner.add_message::<M>();
        self
    }

    /// Registers request type `Q` for the request/response RPC path.
    pub fn add_request<Q: Request>(&mut self) -> &mut Self {
        self.inner.add_request::<Q>();
        self
    }

    /// Low-level primitive: register `C` for wire replication and capture its
    /// snapshot reader.  Both `ProtocolClientExt::add_component` and
    /// `ProtocolServerExt::add_component` call this internally; direct callers
    /// should prefer one of those tier-specific extensions so event installers
    /// are wired in at the same time.
    pub fn register_component<C: Replicate + Component<Mutability = Mutable>>(
        &mut self,
    ) -> &mut Self {
        self.inner.add_component::<C>();
        self.world_data
            .as_mut()
            .expect("shouldn't happen")
            .put_kind::<C>(&ComponentKind::of::<C>());
        self.snapshot_readers.register::<C>();
        self
    }

    /// Return a reference to the `SnapshotReaderRegistry` built up by
    /// `register_component` / `add_component` calls.  Used by the server's
    /// `build_snapshot` helper and by the `#9` desync harness in diax.
    pub fn snapshot_reader_registry(&self) -> &SnapshotReaderRegistry {
        &self.snapshot_readers
    }

    /// Take the `SnapshotReaderRegistry` out of this `Protocol`.  For
    /// consumers that build the registry once and then need to hand it off
    /// without cloning.  Leaves an empty registry in place.
    pub fn take_snapshot_reader_registry(&mut self) -> SnapshotReaderRegistry {
        std::mem::take(&mut self.snapshot_readers)
    }

    /// Register `R` as a Replicated Resource (see `_AGENTS/RESOURCES_PLAN.md`).
    /// Calls `register_component::<R>()` (resources are 1-component entities)
    /// and additionally marks the kind in `resource_kinds` so the
    /// client/server mirror systems recognize incoming resource
    /// components.
    pub fn add_resource<R: crate::ReplicatedResource>(&mut self) -> &mut Self {
        // Component-side registration (also populates world_data).
        self.register_component::<R>();
        // Mark in the resource registry — Replicate-only path on the
        // inner Protocol, not Component-typed.
        self.inner
            .resource_kinds
            .register::<R>(ComponentKind::of::<R>());
        // Mark the kind in `world_data` so the bevy despawn chokepoint
        // (`WorldMut::despawn_entity`) treats the carrier entity as a
        // resource carrier (component-remove, never `World::despawn`).
        self.world_data
            .as_mut()
            .expect("shouldn't happen")
            .mark_resource_kind(&ComponentKind::of::<R>());
        self
    }

    /// Locks the protocol against further registration; later mutating
    /// calls (e.g. `add_plugin`) will panic.
    pub fn lock(&mut self) {
        self.inner.lock();
    }

    /// Consumes this `Protocol`, returning the wrapped `naia_shared::Protocol`.
    pub fn into(self) -> InnerProtocol {
        self.inner
    }

    /// Returns a reference to the wrapped `naia_shared::Protocol`.
    pub fn inner(&self) -> &InnerProtocol {
        &self.inner
    }

    fn check_lock(&self) {
        self.inner.check_lock();
    }

    /// Store a type-erased client-side event-installer closure.  Called by
    /// `ProtocolClientExt::add_component` (in `naia-bevy-client`).  The
    /// closure names no client type, so there is no layering cycle between
    /// `naia-bevy-shared` and `naia-bevy-client`.
    pub fn push_client_event_installer(&mut self, f: EventInstaller) {
        self.client_event_installers.push(f);
    }

    /// Drain and return every installer accumulated by calls to
    /// `push_client_event_installer`.  The caller (`NaiaClientPlugin::<T>::build`)
    /// invokes them after `ComponentEventRegistry<T>` has been inserted into the
    /// `App`, and before the `Protocol` is consumed by `into()`.
    pub fn take_client_event_installers(&mut self) -> Vec<EventInstaller> {
        std::mem::take(&mut self.client_event_installers)
    }

    /// Store a type-erased server-side event-installer closure.  Called by
    /// `ProtocolServerExt::add_component` (in `naia-bevy-server`).  The
    /// closure is `C`-typed at the call site but erased here, so there is no
    /// layering cycle between `naia-bevy-shared` and `naia-bevy-server`.
    pub fn push_server_event_installer(&mut self, f: EventInstaller) {
        self.server_event_installers.push(f);
    }

    /// Drain and return every installer accumulated by calls to
    /// `push_server_event_installer`.  The caller (`NaiaServerPlugin::build`)
    /// invokes them after `ComponentEventRegistry` has been inserted into the
    /// `App`, and before the `Protocol` is consumed by `into()`.
    pub fn take_server_event_installers(&mut self) -> Vec<EventInstaller> {
        std::mem::take(&mut self.server_event_installers)
    }

    /// Takes this `Protocol` out, leaving a default one in its place.
    pub fn build(&mut self) -> Self {
        std::mem::take(self)
    }
}
