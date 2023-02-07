use std::time::Duration;

use naia_socket_shared::SocketConfig;

use crate::{Channel, ChannelDirection, ChannelMode, CompressionConfig, Message, Replicate};

#[derive(Clone)]
pub struct Protocol {
    // Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Option<Duration>,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
}

impl Protocol {
    pub fn builder() -> ProtocolBuilder {
        ProtocolBuilder {
            socket_config: None,
            tick_interval: None,
            compression: None,
        }
    }
}

pub struct ProtocolBuilder {
    socket_config: Option<SocketConfig>,
    tick_interval: Option<Duration>,
    compression: Option<CompressionConfig>,
}

impl ProtocolBuilder {
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        plugin.build(self);
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

    pub fn socket_config(&mut self, config: SocketConfig) -> &mut Self {
        self.socket_config = Some(config);
        self
    }

    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        todo!()
    }

    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        todo!()
    }

    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        todo!()
    }

    pub fn build(&mut self) -> Protocol {
        Protocol {
            socket: self.socket_config.take().unwrap(),
            tick_interval: self.tick_interval.take(),
            compression: self.compression.take(),
        }
    }
}

//Plugin
pub trait Plugin {
    fn build(&self, protocol: &mut ProtocolBuilder);
}
