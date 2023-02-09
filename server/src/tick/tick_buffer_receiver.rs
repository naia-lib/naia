use std::collections::HashMap;

use crate::Events;
use naia_shared::{
    BitReader, ChannelKind, ChannelKinds, ChannelMode, ChannelReader, Message, Serde, SerdeErr,
    Tick,
};

use super::channel_tick_buffer_receiver::ChannelTickBufferReceiver;

pub struct TickBufferReceiver {
    channel_receivers: HashMap<ChannelKind, ChannelTickBufferReceiver>,
}

impl TickBufferReceiver {
    pub fn new() -> Self {
        // initialize receivers
        let mut channel_receivers = HashMap::new();
        for (channel_kind, channel_settings) in ChannelKinds::channels() {
            if let ChannelMode::TickBuffered(_) = channel_settings.mode {
                channel_receivers.insert(channel_kind, ChannelTickBufferReceiver::new());
            }
        }

        Self { channel_receivers }
    }

    // Incoming Messages

    /// Read incoming packet data and store in a buffer
    pub fn read_messages(
        &mut self,
        host_tick: &Tick,
        remote_tick: &Tick,
        channel_reader: &dyn ChannelReader<Box<dyn Message>>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            // read channel index
            let channel_index = ChannelKind::de(reader)?;

            // continue read inside channel
            let channel = self.channel_receivers.get_mut(&channel_index).unwrap();
            channel.read_messages(host_tick, remote_tick, channel_reader, reader)?;
        }

        Ok(())
    }

    /// Retrieved stored data from the tick buffer for the given [`Tick`]
    pub fn receive_messages(
        &mut self,
        host_tick: &Tick,
    ) -> Vec<(ChannelKind, Vec<Box<dyn Message>>)> {
        let mut output = Vec::new();
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages = channel.receive_messages(host_tick);
            output.push((*channel_kind, messages));
        }
        output
    }
}
