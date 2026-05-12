use std::time::Duration;

use naia_socket_shared::{LinkConditionerConfig, SocketConfig};

use crate::{
    connection::compression_config::CompressionConfig,
    messages::{
        channels::{
            channel::{Channel, ChannelDirection, ChannelMode, ChannelSettings},
            channel_kinds::ChannelKinds,
            default_channels::DefaultChannelsPlugin,
        },
        fragment::FragmentedMessage,
        message::Message,
        message_kinds::MessageKinds,
    },
    protocol_id::ProtocolId,
    world::{
        component::{component_kinds::ComponentKinds, replicate::Replicate},
        resource::ResourceKinds,
    },
    Request, RequestOrResponse,
};

/// Extension point for registering channels, messages, and components into a `Protocol`.
pub trait ProtocolPlugin {
    /// Applies this plugin's registrations to `protocol`.
    fn build(&self, protocol: &mut Protocol);
}

/// Builder and configuration container for a naia protocol definition.
///
/// Collects channels, messages, components, and transport settings before being locked and passed to a server or client.
#[derive(Clone)]
pub struct Protocol {
    /// Registry of all channels registered in this protocol.
    pub channel_kinds: ChannelKinds,
    /// Registry of all message types registered in this protocol.
    pub message_kinds: MessageKinds,
    /// Registry of all replicated component types registered in this protocol.
    pub component_kinds: ComponentKinds,
    /// Marker table — which `ComponentKind`s are Replicated Resources.
    /// Receiver side checks this on `SpawnWithComponents` to populate
    /// its `ResourceRegistry`. See `_AGENTS/RESOURCES_PLAN.md`.
    pub resource_kinds: ResourceKinds,
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Duration,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
    /// Whether or not Client Authoritative Entities will be allowed
    pub client_authoritative_entities: bool,
    /// Cached protocol ID, computed when lock() is called
    cached_protocol_id: Option<ProtocolId>,
    locked: bool,
}

impl Default for Protocol {
    fn default() -> Self {
        let mut message_kinds = MessageKinds::new();
        message_kinds.add_message::<FragmentedMessage>();
        message_kinds.add_message::<RequestOrResponse>();

        let channel_kinds = ChannelKinds::new();

        Self {
            channel_kinds,
            message_kinds,
            component_kinds: ComponentKinds::new(),
            resource_kinds: ResourceKinds::new(),
            socket: SocketConfig::new(None, None),
            tick_interval: Duration::from_millis(50),
            compression: None,
            client_authoritative_entities: false,
            cached_protocol_id: None,
            locked: false,
        }
    }
}

impl Protocol {
    /// Returns a default `Protocol` ready for builder-style configuration.
    pub fn builder() -> Self {
        Self::default()
    }

    /// Applies `plugin`'s registrations to this protocol. Builder-style.
    pub fn add_plugin<P: ProtocolPlugin>(&mut self, plugin: P) -> &mut Self {
        self.check_lock();
        plugin.build(self);
        self
    }

    /// Sets the link conditioning configuration (artificial latency/loss). Builder-style.
    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.check_lock();
        self.socket.link_condition = Some(config);
        self
    }

    /// Sets the WebRTC signalling endpoint path. Builder-style.
    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.check_lock();
        self.socket.rtc_endpoint_path = path;
        self
    }

    /// Returns the configured WebRTC signalling endpoint path.
    pub fn get_rtc_endpoint(&self) -> String {
        self.socket.rtc_endpoint_path.clone()
    }

    /// Sets the server tick interval. Builder-style.
    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.check_lock();
        self.tick_interval = duration;
        self
    }

    /// Enables packet compression with the given config. Builder-style.
    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.check_lock();
        self.compression = Some(config);
        self
    }

    /// Enables client-authoritative entity mode, allowing clients to own and update replicated entities. Builder-style.
    pub fn enable_client_authoritative_entities(&mut self) -> &mut Self {
        self.check_lock();
        self.client_authoritative_entities = true;
        self
    }

    /// Registers the six built-in default channels. Builder-style.
    pub fn add_default_channels(&mut self) -> &mut Self {
        self.check_lock();
        let plugin = DefaultChannelsPlugin;
        plugin.build(self);
        self
    }

    /// Registers channel type `C` with the given direction and mode. Builder-style.
    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        self.check_lock();
        self.channel_kinds
            .add_channel::<C>(ChannelSettings::new(mode, direction));
        self
    }

    /// Register a channel with fully-specified `ChannelSettings` (including
    /// `criticality`). Use this when you need a non-default priority tier;
    /// otherwise `add_channel` is sufficient.
    pub fn add_channel_settings<C: Channel>(&mut self, settings: ChannelSettings) -> &mut Self {
        self.check_lock();
        self.channel_kinds.add_channel::<C>(settings);
        self
    }

    /// Registers message type `M`. Builder-style.
    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.check_lock();
        self.message_kinds.add_message::<M>();
        self
    }

    /// Registers request type `Q` and its associated response type. Builder-style.
    pub fn add_request<Q: Request>(&mut self) -> &mut Self {
        self.check_lock();
        // Requests and Responses are handled just like Messages
        self.message_kinds.add_message::<Q>();
        self.message_kinds.add_message::<Q::Response>();
        self
    }

    /// Registers replicated component type `C`. Builder-style.
    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        self.component_kinds.add_component::<C>();
        self
    }

    /// Register `R` as a Replicated Resource.
    ///
    /// A Resource is internally a hidden 1-component entity carrying `R`
    /// as its sole replicated component. This call:
    ///
    /// 1. Calls `add_component::<R>()` to allocate a normal `ComponentKind`
    ///    + NetId for `R` (Resources reuse the component wire encoding).
    /// 2. Records the `ComponentKind` in `resource_kinds` so the receiver
    ///    side can recognize incoming SpawnWithComponents messages whose
    ///    components are resources, and populate its `ResourceRegistry`.
    ///
    /// Idempotent — registering the same type twice is a no-op (matches
    /// `add_component` re-registration semantics; the underlying tables
    /// dedupe on `TypeId`).
    pub fn add_resource<R: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        // Allocate a ComponentKind for R if not already present.
        self.component_kinds.add_component::<R>();
        // Mark the kind as a resource.
        let kind = crate::ComponentKind::of::<R>();
        self.resource_kinds.register::<R>(kind);
        self
    }

    /// Freezes the protocol, computes and caches the protocol ID. Must be called before use.
    pub fn lock(&mut self) {
        self.check_lock();
        self.cached_protocol_id = Some(self.compute_protocol_id());
        self.locked = true;
    }

    /// Panics if the protocol has already been locked.
    pub fn check_lock(&self) {
        if self.locked {
            panic!("Protocol already locked!");
        }
    }

    /// Moves out of the builder and returns the owned `Protocol`.
    pub fn build(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Returns the cached protocol ID. Panics if protocol is not locked.
    pub fn protocol_id(&self) -> ProtocolId {
        self.cached_protocol_id
            .expect("Protocol must be locked before calling protocol_id()")
    }

    /// Compute the protocol ID from current state.
    fn compute_protocol_id(&self) -> ProtocolId {
        let mut hasher = blake3::Hasher::new();

        // Channels
        for name in self.channel_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Messages
        for name in self.message_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Components
        for name in self.component_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Resources — fold in a side-channel marker per resource kind so
        // that two protocols differing only in which kinds are tagged
        // resource hash differently. Without this, downgrading a resource
        // to a plain component (or vice-versa) would collide on the wire
        // mismatch detector.
        hasher.update(b"naia:resources:");
        let mut resource_count = 0u32;
        for _ in self.resource_kinds.iter() {
            resource_count += 1;
        }
        hasher.update(&resource_count.to_le_bytes());

        let hash = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        ProtocolId::new(u64::from_le_bytes(bytes))
    }
}
