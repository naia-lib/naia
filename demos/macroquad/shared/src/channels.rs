use naia_shared::{derive_channels, Channel, ChannelConfig, ChannelMode, ChannelDirection, TickBufferSettings, ReliableSettings};

#[derive_channels]
pub enum Channels {
    PlayerCommand,
    EntityAssignment,
}

pub fn channels_init() -> ChannelConfig<Channels> {
    let mut config = ChannelConfig::new();

    config.add_channel(
        Channels::PlayerCommand, Channel::new(
            ChannelMode::TickBuffered(TickBufferSettings::default()),
            ChannelDirection::ClientToServer),
    );
    config.add_channel(
        Channels::EntityAssignment, Channel::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::ServerToClient),
    );

    config
}