use std::{collections::HashMap, hash::Hash};

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
}

// ChannelIndex
pub trait ChannelIndex: Serde + Eq + Hash {}

// Channel
#[derive(Clone)]
pub struct Channel {
    pub mode: ChannelMode,
    pub direction: ChannelDirection,
}

impl Channel {
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode == ChannelMode::TickBuffered && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        Self { mode, direction }
    }

    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::UnorderedReliable => true,
            ChannelMode::OrderedReliable => true,
            ChannelMode::TickBuffered => false,
        }
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

// ChannelMode
#[derive(Clone, PartialEq)]
pub enum ChannelMode {
    UnorderedUnreliable,
    UnorderedReliable,
    OrderedReliable,
    TickBuffered,
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
            Channel::new(ChannelMode::UnorderedUnreliable, ChannelDirection::Bidirectional),
        );
        config.add_channel(
            DefaultChannels::UnorderedReliable,
            Channel::new(ChannelMode::UnorderedReliable, ChannelDirection::Bidirectional),
        );
        config.add_channel(
            DefaultChannels::OrderedReliable,
            Channel::new(ChannelMode::OrderedReliable, ChannelDirection::Bidirectional),
        );
        config.add_channel(
            DefaultChannels::TickBuffered,
            Channel::new(ChannelMode::TickBuffered, ChannelDirection::ClientToServer),
        );

        config
    }
}
