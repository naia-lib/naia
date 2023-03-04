use naia_serde::{BitReader, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{
    messages::message_kinds::MessageKinds, types::MessageIndex, MessageContainer,
    NetEntityHandleConverter,
};

pub struct IndexedMessageReader;

impl IndexedMessageReader {
    pub fn read_messages(
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
    ) -> Result<Vec<(MessageIndex, MessageContainer)>, SerdeErr> {
        let mut last_read_id: Option<MessageIndex> = None;
        let mut output = Vec::new();

        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            let id_w_msg = Self::read_message(message_kinds, converter, reader, &last_read_id)?;
            last_read_id = Some(id_w_msg.0);
            output.push(id_w_msg);
        }

        Ok(output)
    }

    pub fn read_message_index(
        reader: &mut BitReader,
        last_read_id: &Option<MessageIndex>,
    ) -> Result<MessageIndex, SerdeErr> {
        return if let Some(last_id) = last_read_id {
            let id_diff = UnsignedVariableInteger::<3>::de(reader)?.get() as MessageIndex;
            Ok(last_id.wrapping_add(id_diff))
        } else {
            // read message id
            MessageIndex::de(reader)
        };
    }

    fn read_message(
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
        last_read_id: &Option<MessageIndex>,
    ) -> Result<(MessageIndex, MessageContainer), SerdeErr> {
        // read index
        let message_index = Self::read_message_index(reader, last_read_id)?;

        // read payload
        let new_message = message_kinds.read(reader, converter)?;

        Ok((message_index, new_message))
    }
}
