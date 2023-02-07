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

#[derive(Clone)]
pub struct Protocol {
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Option<Duration>,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
}

impl Protocol {
    pub fn builder() -> ProtocolBuilder {
        ProtocolBuilder {
            link_conditioner_config: None,
            rtc_endpoint_path: None,
            tick_interval: None,
            compression: None,
        }
    }
}

pub struct ProtocolBuilder {
    link_conditioner_config: Option<LinkConditionerConfig>,
    rtc_endpoint_path: Option<String>,
    tick_interval: Option<Duration>,
    compression: Option<CompressionConfig>,
}

impl ProtocolBuilder {
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        plugin.build(self);
        self
    }

    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.link_conditioner_config = Some(config);
        self
    }

    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.rtc_endpoint_path = Some(path);
        self
    }

    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.tick_interval = Some(duration);
        self
    }

    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.compression = Some(config);
        self
    }

    pub fn add_channel<C: Channel + 'static>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        Channels::add_channel::<C>(ChannelSettings::new(mode, direction));
        self
    }

    pub fn add_message<M: Message + 'static>(&mut self) -> &mut Self {
        Messages::add_message::<M>();
        self
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        Components::add_component::<C>();
        self
    }

    pub fn build(&mut self) -> Protocol {
        let socket = SocketConfig::new(
            self.link_conditioner_config.take(),
            self.rtc_endpoint_path.take(),
        );
        Protocol {
            socket,
            tick_interval: self.tick_interval.take(),
            compression: self.compression.take(),
        }
    }
}

//Plugin
pub trait Plugin {
    fn build(&self, protocol: &mut ProtocolBuilder);
}
