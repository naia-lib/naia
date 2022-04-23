use naia_shared::{
    derive_channels, Channel, ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings,
};

#[derive_channels]
pub enum Channels {
    PlayerCommand,
    EntityAssignment,
}

pub const CHANNEL_CONFIG: &[Channel<Channels>] = &[
    Channel {
        index: Channels::PlayerCommand,
        direction: ChannelDirection::ClientToServer,
        mode: ChannelMode::TickBuffered(TickBufferSettings::default()),
    },
    Channel {
        index: Channels::EntityAssignment,
        direction: ChannelDirection::ServerToClient,
        mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
    },
];
