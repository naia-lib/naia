use std::collections::HashMap;

use naia_shared::{
    malformed_drop_warn, BitReader, ChannelKind, ChannelKinds, ChannelMode,
    LocalEntityAndGlobalEntityConverter, MessageContainer, MessageKinds, Serde, SerdeErr, Tick,
};

use crate::connection::tick_buffer_receiver_channel::TickBufferReceiverChannel;

pub struct TickBufferReceiver {
    channel_receivers: HashMap<ChannelKind, TickBufferReceiverChannel>,
}

impl TickBufferReceiver {
    pub fn new(channel_kinds: &ChannelKinds) -> Self {
        // initialize receivers
        let mut channel_receivers = HashMap::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            if let ChannelMode::TickBuffered(settings) = channel_settings.mode {
                channel_receivers.insert(
                    channel_kind,
                    TickBufferReceiverChannel::new(settings.clone()),
                );
            }
        }

        Self { channel_receivers }
    }

    // Incoming Messages

    /// Read incoming packet data and store in a buffer
    pub fn read_messages(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        host_tick: Tick,
        remote_tick: Tick,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            // read channel index
            let channel_kind = ChannelKind::de(channel_kinds, reader)?;

            // continue read inside channel
            // [115662-3] common-02: a channel kind with no local tick buffer
            // (unconfigured channel, or config mismatch with the sender)
            // fails the packet read instead of panicking, mirroring the
            // sibling message path in message_manager.rs. The caller drops
            // the packet; the connection stays up and a later tick replaces
            // the lost samples.
            let Some(channel) = self.channel_receivers.get_mut(&channel_kind) else {
                malformed_drop_warn(format!(
                    "dropping tick-buffered data for unconfigured channel {channel_kind:?}"
                ));
                return Err(SerdeErr);
            };
            channel.read_messages(converter, message_kinds, host_tick, remote_tick, reader)?;
        }

        Ok(())
    }

    /// Directly inject a message into the specified channel's tick buffer (test_utils only)
    #[cfg(feature = "test_utils")]
    pub fn inject_message(
        &mut self,
        channel_kind: &ChannelKind,
        host_tick: &Tick,
        message_tick: &Tick,
        message: MessageContainer,
    ) -> bool {
        if let Some(channel) = self.channel_receivers.get_mut(channel_kind) {
            channel.inject_message(host_tick, message_tick, message)
        } else {
            false
        }
    }

    /// Retrieved stored data from the tick buffer for the given [`Tick`]
    pub fn receive_messages(
        &mut self,
        host_tick: Tick,
    ) -> Vec<(ChannelKind, Vec<MessageContainer>)> {
        let mut output = Vec::new();
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages = channel.receive_messages(host_tick);
            output.push((*channel_kind, messages));
        }
        output
    }
}

// ---- [115662-3] RED: a tick-buffer read naming a channel whose mode is
// not tick-buffered. The kind decodes fine (it is registered) but has no
// tick-buffer receiver; pre-fix the lookup unwraps and panics, post-fix it
// returns `Err` so the caller drops the packet.
#[cfg(test)]
mod wrong_channel_tests {
    use naia_shared::{
        default_channels::{TickBufferedChannel, UnorderedReliableChannel},
        BitReader, BitWrite, BitWriter, ChannelDirection, ChannelKinds, ChannelMode,
        ChannelSettings, FakeEntityConverter, MessageKinds, ReliableSettings, TickBufferSettings,
    };

    use super::*;

    fn kinds_with_tick_buffered_plus_unreliable() -> ChannelKinds {
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<TickBufferedChannel>(ChannelSettings::new(
            ChannelMode::TickBuffered(TickBufferSettings::default()),
            ChannelDirection::ClientToServer,
        ));
        kinds.add_channel::<UnorderedReliableChannel>(ChannelSettings::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::ClientToServer,
        ));
        kinds
    }

    /// `channel_continue = true`, then net_id 1 (the non-tick-buffered
    /// channel; two registered channels fit in one bit).
    fn rogue_channel_packet() -> Vec<u8> {
        let mut writer = BitWriter::new();
        writer.write_bit(true);
        writer.write_bit(true);
        writer.to_packet().slice().to_vec()
    }

    #[test]
    fn message_for_a_non_tick_buffered_channel_is_dropped_without_panic() {
        // common-02: remote input must not panic. A tick-buffer read that
        // names a live but non-tick-buffered channel fails closed; the
        // caller drops the packet like any other undecodable inbound.
        let kinds = kinds_with_tick_buffered_plus_unreliable();
        let mut receiver = TickBufferReceiver::new(&kinds);
        let message_kinds = MessageKinds::new();
        let bytes = rogue_channel_packet();
        let mut reader = BitReader::new(&bytes);
        let result = receiver.read_messages(
            &kinds,
            &message_kinds,
            0,
            0,
            &FakeEntityConverter,
            &mut reader,
        );
        assert!(
            result.is_err(),
            "a tick-buffer read naming a non-tick-buffered channel must fail closed, never panic",
        );
    }
}
