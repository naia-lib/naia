use std::collections::HashMap;

use naia_shared::{
    serde::{BitReader, Serde, SerdeErr},
    ChannelConfig, ChannelIndex, ChannelMode, ChannelReader, Protocolize, Tick,
};

use super::channel_tick_buffer_receiver::ChannelTickBufferReceiver;

pub struct TickBufferReceiver<P: Protocolize, C: ChannelIndex> {
    channel_receivers: HashMap<C, ChannelTickBufferReceiver<P>>,
}

impl<P: Protocolize, C: ChannelIndex> TickBufferReceiver<P, C> {
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        // initialize receivers
        let mut channel_receivers = HashMap::new();
        for (channel_index, channel) in channel_config.channels() {
            if let ChannelMode::TickBuffered(_) = channel.mode {
                channel_receivers.insert(channel_index.clone(), ChannelTickBufferReceiver::new());
            }
        }

        Self { channel_receivers }
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        host_tick: &Tick,
        remote_tick: &Tick,
        channel_reader: &dyn ChannelReader<P>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            // read channel index
            let channel_index = C::de(reader)?;

            // continue read inside channel
            let channel = self.channel_receivers.get_mut(&channel_index).unwrap();
            channel.read_messages(host_tick, remote_tick, channel_reader, reader)?;
        }

        Ok(())
    }

    pub fn receive_messages(&mut self, host_tick: &Tick) -> Vec<(C, P)> {
        let mut output = Vec::new();
        for (channel_index, channel) in &mut self.channel_receivers {
            let mut messages = channel.receive_messages(host_tick);
            for message in messages.drain(..) {
                output.push((channel_index.clone(), message));
            }
        }
        output
    }
}
