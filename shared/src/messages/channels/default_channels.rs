use crate::{
    messages::channels::channel::{
        ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings,
    },
    Protocol, ProtocolPlugin,
};

use naia_derive::ChannelInternal;

#[derive(ChannelInternal)]
pub struct UnorderedUnreliableChannel;
#[derive(ChannelInternal)]
pub struct SequencedUnreliableChannel;
#[derive(ChannelInternal)]
pub struct UnorderedReliableChannel;
#[derive(ChannelInternal)]
pub struct SequencedReliableChannel;
#[derive(ChannelInternal)]
pub struct OrderedReliableChannel;
#[derive(ChannelInternal)]
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
