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
}

impl Channel {
    pub fn new(mode: ChannelMode) -> Self {
        Self { mode }
    }

    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::UnorderedReliable => true,
            ChannelMode::OrderedReliable => true,
            ChannelMode::TickBuffered => false,
        }
    }
}

// ChannelMode
#[derive(Clone)]
pub enum ChannelMode {
    UnorderedUnreliable,
    UnorderedReliable,
    OrderedReliable,
    TickBuffered,
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
            Channel::new(ChannelMode::UnorderedUnreliable),
        );
        config.add_channel(
            DefaultChannels::UnorderedReliable,
            Channel::new(ChannelMode::UnorderedReliable),
        );
        config.add_channel(
            DefaultChannels::OrderedReliable,
            Channel::new(ChannelMode::OrderedReliable),
        );
        config.add_channel(
            DefaultChannels::TickBuffered,
            Channel::new(ChannelMode::TickBuffered),
        );

        config
    }
}
