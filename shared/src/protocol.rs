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
    world::component::{component_kinds::ComponentKinds, replicate::Replicate},
    Request, RequestOrResponse,
};

// Protocol Plugin
pub trait ProtocolPlugin {
    fn build(&self, protocol: &mut Protocol);
}

// Protocol
#[derive(Clone)]
pub struct Protocol {
    pub channel_kinds: ChannelKinds,
    pub message_kinds: MessageKinds,
    pub component_kinds: ComponentKinds,
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
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn add_plugin<P: ProtocolPlugin>(&mut self, plugin: P) -> &mut Self {
        self.check_lock();
        plugin.build(self);
        self
    }

    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.check_lock();
        self.socket.link_condition = Some(config);
        self
    }

    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.check_lock();
        self.socket.rtc_endpoint_path = path;
        self
    }

    pub fn get_rtc_endpoint(&self) -> String {
        self.socket.rtc_endpoint_path.clone()
    }

    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.check_lock();
        self.tick_interval = duration;
        self
    }

    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.check_lock();
        self.compression = Some(config);
        self
    }

    pub fn enable_client_authoritative_entities(&mut self) -> &mut Self {
        self.check_lock();
        self.client_authoritative_entities = true;
        self
    }

    pub fn add_default_channels(&mut self) -> &mut Self {
        self.check_lock();
        let plugin = DefaultChannelsPlugin;
        plugin.build(self);
        self
    }

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

    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.check_lock();
        self.message_kinds.add_message::<M>();
        self
    }

    pub fn add_request<Q: Request>(&mut self) -> &mut Self {
        self.check_lock();
        // Requests and Responses are handled just like Messages
        self.message_kinds.add_message::<Q>();
        self.message_kinds.add_message::<Q::Response>();
        self
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        self.component_kinds.add_component::<C>();
        self
    }

    pub fn lock(&mut self) {
        self.check_lock();
        self.cached_protocol_id = Some(self.compute_protocol_id());
        self.locked = true;
    }

    pub fn check_lock(&self) {
        if self.locked {
            panic!("Protocol already locked!");
        }
    }

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

        let hash = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        ProtocolId::new(u64::from_le_bytes(bytes))
    }
}
