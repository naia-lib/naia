use std::marker::PhantomData;

use naia_serde::{BitReader, Serde, UnsignedVariableInteger};

use crate::{
    messages::{message_channel::ChannelReader, message_list_header},
    types::MessageId,
};

pub struct ReliableReceiver<P> {
    phantom_p: PhantomData<P>,
}

impl<P> ReliableReceiver<P> {
    pub fn read_incoming_messages(
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
    ) -> Vec<(MessageId, P)> {
        let message_count = message_list_header::read(bit_reader);

        let mut last_read_id: Option<MessageId> = None;
        let mut output = Vec::new();

        for _x in 0..message_count {
            let id_w_msg = Self::read_incoming_message(channel_reader, bit_reader, &last_read_id);
            last_read_id = Some(id_w_msg.0);
            output.push(id_w_msg);
        }
        output
    }

    fn read_incoming_message(
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
        last_read_id: &Option<MessageId>,
    ) -> (MessageId, P) {
        let message_id: MessageId = if let Some(last_id) = last_read_id {
            let id_diff = UnsignedVariableInteger::<3>::de(bit_reader).unwrap().get() as MessageId;
            last_id.wrapping_add(id_diff)
        } else {
            // read message id
            MessageId::de(bit_reader).unwrap()
        };

        // read payload
        let new_message = channel_reader.read(bit_reader);

        (message_id, new_message)
    }
}
