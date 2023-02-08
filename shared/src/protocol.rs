use std::time::Duration;

use naia_socket_shared::{LinkConditionerConfig, SocketConfig};

use crate::{
    component::replicate::{Components, Replicate},
    connection::compression_config::CompressionConfig,
    messages::{
        channel_config::{Channel, ChannelDirection, ChannelMode, ChannelSettings, Channels},
        message::{Message, Messages},
    },
};

// Protocol Plugin
pub trait Plugin {
    fn build(&self, protocol: &mut Protocol);
}

// Protocol
pub struct Protocol {
    pub channels: Channels,
    pub messages: Messages,
    pub components: Components,
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Option<Duration>,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
    locked: bool,
}

impl Protocol {
    pub fn new() -> Protocol {
        Protocol {
            channels: Channels::new(),
            messages: Messages::new(),
            components: Components::new(),
            socket: SocketConfig::new(None, None),
            tick_interval: None,
            compression: None,
            locked: false,
        }
    }

    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
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

    pub fn add_channel<C: Channel + 'static>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        self.check_lock();
        self.channels.add_channel::<C>(ChannelSettings::new(mode, direction));
        self
    }

    pub fn add_message<M: Message + 'static>(&mut self) -> &mut Self {
        self.check_lock();
        self.messages.add_message::<M>();
        self
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        self.components.add_component::<C>();
        self
    }

    pub fn lock(&mut self) {
        self.check_lock();
        self.locked = true;
    }

    fn check_lock(&self) {
        if self.locked {
            panic!("Protocol already locked!");
        }
    }
}


