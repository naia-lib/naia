use std::collections::hash_map::IntoIter;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use lazy_static::lazy_static;

use crate::types::ChannelId;

// Channels
pub struct Channels {
    current_id: u16,
    type_to_id_map: HashMap<TypeId, ChannelId>,
    id_to_data_map: HashMap<ChannelId, (TypeId, ChannelSettings)>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            current_id: 0,
            type_to_id_map: HashMap::new(),
            id_to_data_map: HashMap::new(),
        }
    }

    pub fn add_channel<C: Channel>(&mut self, settings: ChannelSettings) {
        let type_id = TypeId::of::<C>();
        let channel_id = ChannelId::new(self.current_id);
        self.type_to_id_map.insert(type_id, channel_id);
        self.id_to_data_map.insert(channel_id, (type_id, settings));
        self.current_id += 1;
        //TODO: check for current_id overflow?
    }

    pub fn kind_to_type(&self, channel_kind: &ChannelId) -> TypeId {
        return self.id_to_data_map.get(channel_kind).expect("Must properly initialize Channel with Protocol via `add_channel()` function!").0;
    }

    pub fn type_to_id<C: Channel>(&self) -> ChannelId {
        let type_id = TypeId::of::<C>();
        return *self.type_to_id_map
            .get(&type_id)
            .expect("Must properly initialize Channel with Protocol via `add_channel()` function!");
    }

    pub fn channels(&self) -> Vec<(ChannelId, ChannelSettings)> {
        // TODO: is there a better way to do this without copying + cloning?
        // How to return a reference here (behind a Mutex ..)
        let mut output = Vec::new();
        for (id, (_, settings)) in &self.id_to_data_map {
            output.push((*id, settings.clone()));
        }
        output
    }

    pub fn channel(&self, id: &ChannelId) -> ChannelSettings {
        let (_, settings) = self.id_to_data_map.get(id).unwrap();
        settings.clone()
    }
}

// Channel Trait
pub trait Channel: 'static {}

// ChannelSettings
#[derive(Clone)]
pub struct ChannelSettings {
    pub mode: ChannelMode,
    pub direction: ChannelDirection,
}

impl ChannelSettings {
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode.tick_buffered() && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        Self { mode, direction }
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

// TODO: Connor, reimplement!
// // Default Channels
//
// mod define_default_channels {
//     use super::ChannelIndex;
//     use crate::{derive_serde, serde};
//
//     #[derive(Eq, Hash)]
//     #[derive_serde]
//     pub enum DefaultChannels {
//         UnorderedUnreliable,
//         SequencedUnreliable,
//         UnorderedReliable,
//         SequencedReliable,
//         OrderedReliable,
//         TickBuffered,
//     }
//
//     impl ChannelIndex for DefaultChannels {}
// }
// pub use define_default_channels::DefaultChannels;
// use crate::types::ChannelId;
//
// impl ChannelConfig<DefaultChannels> {
//     pub fn default() -> &'static [Channel<DefaultChannels>] {
//         DEFAULT_CHANNEL_CONFIG
//     }
// }
//
// const DEFAULT_CHANNEL_CONFIG: &[Channel<DefaultChannels>] = &[
//     Channel {
//         index: DefaultChannels::UnorderedUnreliable,
//         direction: ChannelDirection::Bidirectional,
//         mode: ChannelMode::UnorderedUnreliable,
//     },
//     Channel {
//         index: DefaultChannels::SequencedUnreliable,
//         direction: ChannelDirection::Bidirectional,
//         mode: ChannelMode::SequencedUnreliable,
//     },
//     Channel {
//         index: DefaultChannels::UnorderedReliable,
//         direction: ChannelDirection::Bidirectional,
//         mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
//     },
//     Channel {
//         index: DefaultChannels::SequencedReliable,
//         direction: ChannelDirection::Bidirectional,
//         mode: ChannelMode::SequencedReliable(ReliableSettings::default()),
//     },
//     Channel {
//         index: DefaultChannels::OrderedReliable,
//         direction: ChannelDirection::Bidirectional,
//         mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
//     },
//     Channel {
//         index: DefaultChannels::TickBuffered,
//         direction: ChannelDirection::ClientToServer,
//         mode: ChannelMode::TickBuffered(TickBufferSettings::default()),
//     },
// ];
