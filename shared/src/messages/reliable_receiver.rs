use std::marker::PhantomData;

use naia_serde::{BitReader, Serde, SerdeErr, UnsignedVariableInteger};

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
        reader: &mut BitReader,
    ) -> Result<Vec<(MessageId, P)>, SerdeErr> {
        let message_count = message_list_header::read(reader)?;

        let mut last_read_id: Option<MessageId> = None;
        let mut output = Vec::new();

        for _x in 0..message_count {
            let id_w_msg = Self::read_incoming_message(channel_reader, reader, &last_read_id)?;
            last_read_id = Some(id_w_msg.0);
            output.push(id_w_msg);
        }
        Ok(output)
    }

    fn read_incoming_message(
        channel_reader: &dyn ChannelReader<P>,
        reader: &mut BitReader,
        last_read_id: &Option<MessageId>,
    ) -> Result<(MessageId, P), SerdeErr> {
        let message_id: MessageId = if let Some(last_id) = last_read_id {
            let id_diff = UnsignedVariableInteger::<3>::de(reader)?.get() as MessageId;
            last_id.wrapping_add(id_diff)
        } else {
            // read message id
            MessageId::de(reader)?
        };

        // read payload
        let new_message = channel_reader.read(reader)?;

        Ok((message_id, new_message))
    }
}
