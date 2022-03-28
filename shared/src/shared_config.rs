use std::{default::Default, time::Duration};

use naia_socket_shared::SocketConfig;

use crate::{
    connection::compression_config::CompressionConfig,
    messages::channel_config::{ChannelConfig, ChannelIndex, DefaultChannels},
};

/// Contains Config properties which will be shared by Server and Client
#[derive(Clone)]
pub struct SharedConfig<C: ChannelIndex> {
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// Config for Message channels
    pub channel: ChannelConfig<C>,
    /// The duration between each tick
    pub tick_interval: Option<Duration>,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
}

impl<C: ChannelIndex> SharedConfig<C> {
    /// Creates a new SharedConfig
    pub fn new(
        socket: SocketConfig,
        channel: ChannelConfig<C>,
        tick_interval: Option<Duration>,
        compression: Option<CompressionConfig>,
    ) -> Self {
        Self {
            socket,
            channel,
            tick_interval,
            compression,
        }
    }
}

impl SharedConfig<DefaultChannels> {
    /// Creates a new with Default parameters
    pub fn default() -> Self {
        Self {
            socket: SocketConfig::default(),
            channel: ChannelConfig::<DefaultChannels>::default(),
            tick_interval: Some(Duration::from_millis(50)),
            compression: None,
        }
    }
}
