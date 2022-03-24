use std::{collections::HashMap, hash::Hash, time::Duration};

use crate::{derive_serde, serde, serde::Serde};

// ChannelConfig
#[derive(Clone)]
pub struct ChannelConfig<C: ChannelIndex> {
    map: HashMap<C, Channel>,
}

impl<C: ChannelIndex> ChannelConfig<C> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn add_channel(&mut self, channel_index: C, channel: Channel) {
        self.map.insert(channel_index, channel);
    }

    pub fn settings(&self, channel_index: &C) -> &Channel {
        return self
            .map
            .get(channel_index)
            .expect("Channel has not been registered in the config!");
    }

    pub fn all_tick_buffer_settings(&self) -> Vec<(C, TickBufferSettings)> {
        let mut output = Vec::new();

        for (index, channel) in self.map.iter() {
            if let ChannelMode::TickBuffered(settings) = &channel.mode {
                output.push((index.clone(), settings.clone()));
            }
        }

        output
    }

    pub fn all_channels(&self) -> Vec<(C, Channel)> {
        let mut output = Vec::new();

        for (index, channel) in self.map.iter() {
            output.push((index.clone(), channel.clone()));
        }

        output
    }
}

// ChannelIndex
pub trait ChannelIndex: 'static + Serde + Eq + Hash {}

// Channel
#[derive(Clone)]
pub struct Channel {
    pub mode: ChannelMode,
    direction: ChannelDirection,
}

impl Channel {
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode.tick_buffered() && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        Self { mode, direction }
    }

    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::UnorderedReliable(_) => true,
            ChannelMode::OrderedReliable(_) => true,
            ChannelMode::TickBuffered(_) => false,
        }
    }

    pub fn tick_buffered(&self) -> bool {
        return self.mode.tick_buffered();
    }

    pub fn can_send_to_server(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => true,
            ChannelDirection::ServerToClient => false,
            ChannelDirection::Bidirectional => true,
        }
    }

    pub fn can_send_to_client(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => false,
            ChannelDirection::ServerToClient => true,
            ChannelDirection::Bidirectional => true,
        }
    }
}

#[derive(Clone)]
pub struct ReliableSettings {
    pub rtt_resend_factor: f32,
}

impl Default for ReliableSettings {
    fn default() -> Self {
        Self {
            rtt_resend_factor: 1.5,
        }
    }
}

#[derive(Clone)]
pub struct TickBufferSettings {
    pub resend_interval: Duration,
}

impl Default for TickBufferSettings {
    fn default() -> Self {
        Self {
            resend_interval: Duration::from_millis(100),
        }
    }
}

// ChannelMode
#[derive(Clone)]
pub enum ChannelMode {
    UnorderedUnreliable,
    UnorderedReliable(ReliableSettings),
    OrderedReliable(ReliableSettings),
    TickBuffered(TickBufferSettings),
}

impl ChannelMode {
    pub fn tick_buffered(&self) -> bool {
        match self {
            ChannelMode::TickBuffered(_) => true,
            _ => false,
        }
    }
}

// ChannelDirection
#[derive(Clone, PartialEq)]
pub enum ChannelDirection {
    ClientToServer,
    ServerToClient,
    Bidirectional,
}

// Default Channels
#[derive(Eq, Hash)]
#[derive_serde]
pub enum DefaultChannels {
    UnorderedUnreliable,
    UnorderedReliable,
    OrderedReliable,
    TickBuffered,
}

impl ChannelIndex for DefaultChannels {}

impl ChannelConfig<DefaultChannels> {
    pub fn default() -> Self {
        let mut config = ChannelConfig::new();

        config.add_channel(
            DefaultChannels::UnorderedUnreliable,
            Channel::new(
                ChannelMode::UnorderedUnreliable,
                ChannelDirection::Bidirectional,
            ),
        );
        config.add_channel(
            DefaultChannels::UnorderedReliable,
            Channel::new(
                ChannelMode::UnorderedReliable(ReliableSettings::default()),
                ChannelDirection::Bidirectional,
            ),
        );
        config.add_channel(
            DefaultChannels::OrderedReliable,
            Channel::new(
                ChannelMode::OrderedReliable(ReliableSettings::default()),
                ChannelDirection::Bidirectional,
            ),
        );
        config.add_channel(
            DefaultChannels::TickBuffered,
            Channel::new(
                ChannelMode::TickBuffered(TickBufferSettings::default()),
                ChannelDirection::ClientToServer,
            ),
        );

        config
    }
}
