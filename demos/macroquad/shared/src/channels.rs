use naia_shared::{Channel, ChannelMode, ChannelConfig, derive_channels};

#[derive_channels]
pub enum Channels {
    PlayerCommand,
    EntityAssignment,
}

pub fn channels_init() -> ChannelConfig<Channels> {
    let mut config = ChannelConfig::new();

    config.add_channel(Channels::PlayerCommand, Channel::new(ChannelMode::TickBuffered));
    config.add_channel(Channels::EntityAssignment, Channel::new(ChannelMode::UnorderedReliable));

    config
}