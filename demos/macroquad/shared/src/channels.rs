use naia_shared::{
    derive_channels, Channel, ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings,
};

#[derive_channels]
pub enum Channels {
    PlayerCommand,
    EntityAssignment,
}

// TODO: link these in with the enum and config
pub struct PlayerCommandChannel;
pub struct EntityAssignmentChannel;

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
