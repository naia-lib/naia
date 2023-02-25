use crate::{
    messages::channel::TickBufferSettings, Channel, ChannelDirection, ChannelMode, Protocol,
    ProtocolPlugin, ReliableSettings,
};

#[derive(Channel)]
pub struct UnorderedUnreliableChannel;
#[derive(Channel)]
pub struct SequencedUnreliableChannel;
#[derive(Channel)]
pub struct UnorderedReliableChannel;
#[derive(Channel)]
pub struct SequencedReliableChannel;
#[derive(Channel)]
pub struct OrderedReliableChannel;
#[derive(Channel)]
pub struct TickBufferedChannel;

pub(crate) struct DefaultChannelsPlugin;
impl ProtocolPlugin for DefaultChannelsPlugin {
    fn build(&self, protocol: &mut Protocol) {
        protocol
            .add_channel::<UnorderedUnreliableChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            )
            .add_channel::<SequencedUnreliableChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::SequencedUnreliable,
            )
            .add_channel::<UnorderedReliableChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedReliable(ReliableSettings::default()),
            )
            .add_channel::<SequencedReliableChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::SequencedReliable(ReliableSettings::default()),
            )
            .add_channel::<OrderedReliableChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::OrderedReliable(ReliableSettings::default()),
            )
            .add_channel::<TickBufferedChannel>(
                ChannelDirection::ClientToServer,
                ChannelMode::TickBuffered(TickBufferSettings::default()),
            );
    }
}
