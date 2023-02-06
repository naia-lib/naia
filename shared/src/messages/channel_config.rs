use std::{collections::HashMap, hash::Hash};

use crate::serde::Serde;

// ChannelConfig
#[derive(Clone)]
pub struct ChannelConfig<C: ChannelIndex> {
    channels: HashMap<C, Channel<C>>,
}

impl<C: ChannelIndex> ChannelConfig<C> {
    pub fn new(input: &[Channel<C>]) -> Self {
        let mut new_me = Self {
            channels: HashMap::new(),
        };

        for channel in input {
            new_me.add_channel(channel.clone());
        }

        new_me
    }

    fn add_channel(&mut self, channel: Channel<C>) {
        self.channels.insert(channel.index.clone(), channel);
    }

    pub fn channel(&self, channel_index: &C) -> &Channel<C> {
        return self
            .channels
            .get(channel_index)
            .expect("Channel has not been registered in the config!");
    }

    pub fn channels(&self) -> &HashMap<C, Channel<C>> {
        &self.channels
    }
}

// ChannelIndex
pub trait ChannelIndex: Serde + Eq + Hash + Sized + Sync + Send + 'static {}

// Channel
#[derive(Clone)]
pub struct Channel<C: ChannelIndex> {
    pub index: C,
    pub mode: ChannelMode,
    pub direction: ChannelDirection,
}

impl<C: ChannelIndex> Channel<C> {
    pub fn new(index: C, mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode.tick_buffered() && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        Self {
            index,
            mode,
            direction,
        }
    }

    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::SequencedUnreliable => false,
            ChannelMode::UnorderedReliable(_) => true,
            ChannelMode::SequencedReliable(_) => true,
            ChannelMode::OrderedReliable(_) => true,
            ChannelMode::TickBuffered(_) => false,
        }
    }

    pub fn tick_buffered(&self) -> bool {
        self.mode.tick_buffered()
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

impl ReliableSettings {
    pub const fn default() -> Self {
        Self {
            rtt_resend_factor: 1.5,
        }
    }
}

#[derive(Clone)]
pub struct TickBufferSettings {
    pub tick_resend_factor: u8,
}

impl TickBufferSettings {
    pub const fn default() -> Self {
        Self {
            tick_resend_factor: 1,
        }
    }
}

// ChannelMode
#[derive(Clone)]
pub enum ChannelMode {
    UnorderedUnreliable,
    SequencedUnreliable,
    UnorderedReliable(ReliableSettings),
    SequencedReliable(ReliableSettings),
    OrderedReliable(ReliableSettings),
    TickBuffered(TickBufferSettings),
}

impl ChannelMode {
    pub fn tick_buffered(&self) -> bool {
        matches!(self, ChannelMode::TickBuffered(_))
    }
}

// ChannelDirection
#[derive(Clone, Eq, PartialEq)]
pub enum ChannelDirection {
    ClientToServer,
    ServerToClient,
    Bidirectional,
}

// Default Channels

mod define_default_channels {
    use super::ChannelIndex;
    use crate::serde::Serde;

    #[derive(Eq, Hash, PartialEq, Clone, Serde)]
    pub enum DefaultChannels {
        UnorderedUnreliable,
        SequencedUnreliable,
        UnorderedReliable,
        SequencedReliable,
        OrderedReliable,
        TickBuffered,
    }

    impl ChannelIndex for DefaultChannels {}
}
pub use define_default_channels::DefaultChannels;

impl ChannelConfig<DefaultChannels> {
    pub fn default() -> &'static [Channel<DefaultChannels>] {
        DEFAULT_CHANNEL_CONFIG
    }
}

const DEFAULT_CHANNEL_CONFIG: &[Channel<DefaultChannels>] = &[
    Channel {
        index: DefaultChannels::UnorderedUnreliable,
        direction: ChannelDirection::Bidirectional,
        mode: ChannelMode::UnorderedUnreliable,
    },
    Channel {
        index: DefaultChannels::SequencedUnreliable,
        direction: ChannelDirection::Bidirectional,
        mode: ChannelMode::SequencedUnreliable,
    },
    Channel {
        index: DefaultChannels::UnorderedReliable,
        direction: ChannelDirection::Bidirectional,
        mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
    },
    Channel {
        index: DefaultChannels::SequencedReliable,
        direction: ChannelDirection::Bidirectional,
        mode: ChannelMode::SequencedReliable(ReliableSettings::default()),
    },
    Channel {
        index: DefaultChannels::OrderedReliable,
        direction: ChannelDirection::Bidirectional,
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
    },
    Channel {
        index: DefaultChannels::TickBuffered,
        direction: ChannelDirection::ClientToServer,
        mode: ChannelMode::TickBuffered(TickBufferSettings::default()),
    },
];
