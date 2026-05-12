use crate::{
    messages::channels::channel::{
        ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings,
    },
    Protocol, ProtocolPlugin,
};

use naia_derive::ChannelInternal;

/// Built-in bidirectional unordered-unreliable channel.
#[derive(ChannelInternal)]
pub struct UnorderedUnreliableChannel;
/// Built-in bidirectional sequenced-unreliable channel.
#[derive(ChannelInternal)]
pub struct SequencedUnreliableChannel;
/// Built-in bidirectional unordered-reliable channel.
#[derive(ChannelInternal)]
pub struct UnorderedReliableChannel;
/// Built-in bidirectional sequenced-reliable channel.
#[derive(ChannelInternal)]
pub struct SequencedReliableChannel;
/// Built-in bidirectional ordered-reliable channel.
#[derive(ChannelInternal)]
pub struct OrderedReliableChannel;
/// Built-in client-to-server tick-buffered channel.
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
