
use std::time::Duration;

use naia_socket_shared::{LinkConditionerConfig, SocketConfig};

use crate::{
    component::{component_kinds::ComponentKinds, replicate::Replicate},
    connection::compression_config::CompressionConfig,
    messages::{
        channel::{Channel, ChannelDirection, ChannelMode, ChannelSettings},
        channel_kinds::ChannelKinds,
        default_channels::DefaultChannelsPlugin,
        message::Message,
        message_kinds::MessageKinds,
    },
};

// Protocol Plugin
pub trait ProtocolPlugin {
    fn build(&self, protocol: &mut Protocol);
}

// Protocol
pub struct Protocol {
    pub channel_kinds: ChannelKinds,
    pub message_kinds: MessageKinds,
    pub component_kinds: ComponentKinds,
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Option<Duration>,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
    locked: bool,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            channel_kinds: ChannelKinds::new(),
            message_kinds: MessageKinds::new(),
            component_kinds: ComponentKinds::new(),
            socket: SocketConfig::new(None, None),
            tick_interval: None,
            compression: None,
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

    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.check_lock();
        self.tick_interval = Some(duration);
        self
    }

    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.check_lock();
        self.compression = Some(config);
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

    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.check_lock();
        self.message_kinds.add_message::<M>();
        self
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        self.component_kinds.add_component::<C>();
        self
    }

    pub fn lock(&mut self) {
        self.check_lock();
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
}
