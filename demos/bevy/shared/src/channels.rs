use naia_shared::{Channel, ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings};
use naia_bevy_shared::{ProtocolPlugin, Protocol};

#[derive(Channel)]
pub struct PlayerCommandChannel;

#[derive(Channel)]
pub struct EntityAssignmentChannel;

// Plugin
pub struct ChannelsPlugin;

impl ProtocolPlugin for ChannelsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_channel::<PlayerCommandChannel>(
                ChannelDirection::ClientToServer,
                ChannelMode::TickBuffered(TickBufferSettings::default()),
            )
            .add_channel::<EntityAssignmentChannel>(
                ChannelDirection::ServerToClient,
                ChannelMode::UnorderedReliable(ReliableSettings::default()),
            );
    }
}
