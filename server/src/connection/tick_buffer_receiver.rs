use std::{collections::HashMap, hash::Hash};

use naia_shared::{
    BitReader, ChannelKind, ChannelKinds, ChannelMode, EntityAndGlobalEntityConverter,
    EntityAndLocalEntityConverter, EntityConverter, MessageContainer, Protocol, Serde, SerdeErr,
    Tick,
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
    pub fn read_messages<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        protocol: &Protocol,
        host_tick: &Tick,
        remote_tick: &Tick,
        global_world_manager: &dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &dyn EntityAndLocalEntityConverter<E>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let converter = EntityConverter::new(global_world_manager, local_world_manager);
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            // read channel index
            let channel_kind = ChannelKind::de(&protocol.channel_kinds, reader)?;

            // continue read inside channel
            let channel = self.channel_receivers.get_mut(&channel_kind).unwrap();
            channel.read_messages(
                &converter,
                &protocol.message_kinds,
                host_tick,
                remote_tick,
                reader,
            )?;
        }

        Ok(())
    }

    /// Retrieved stored data from the tick buffer for the given [`Tick`]
    pub fn receive_messages(
        &mut self,
        host_tick: &Tick,
    ) -> Vec<(ChannelKind, Vec<MessageContainer>)> {
        let mut output = Vec::new();
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages = channel.receive_messages(host_tick);
            output.push((*channel_kind, messages));
        }
        output
    }
}
