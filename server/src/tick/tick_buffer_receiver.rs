use std::collections::HashMap;

use crate::Events;
use naia_shared::{
    BitReader, ChannelId, ChannelMode, ChannelReader, Channels, Message, Serde, SerdeErr, Tick,
};

use super::channel_tick_buffer_receiver::ChannelTickBufferReceiver;

pub struct TickBufferReceiver {
    channel_receivers: HashMap<ChannelId, ChannelTickBufferReceiver>,
}

impl TickBufferReceiver {
    pub fn new() -> Self {
        // initialize receivers
        let mut channel_receivers = HashMap::new();
        for (channel_id, channel_settings) in Channels::channels() {
            if let ChannelMode::TickBuffered(_) = channel_settings.mode {
                channel_receivers.insert(channel_id, ChannelTickBufferReceiver::new());
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
            let channel_index = ChannelId::de(reader)?;

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
    ) -> Vec<(ChannelId, Vec<Box<dyn Message>>)> {
        let mut output = Vec::new();
        for (channel_id, channel) in &mut self.channel_receivers {
            let messages = channel.receive_messages(host_tick);
            output.push((*channel_id, messages));
        }
        output
    }
}
