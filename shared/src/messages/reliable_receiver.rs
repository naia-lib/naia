use std::marker::PhantomData;

use naia_serde::{BitReader, Serde, UnsignedVariableInteger};

use crate::{
    messages::message_list_header,
    protocol::{
        entity_property::NetEntityHandleConverter, protocolize::Protocolize,
    },
    types::MessageId,
};

pub struct ReliableReceiver<P: Protocolize> {
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize> ReliableReceiver<P> {
    pub fn read_incoming_messages(
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Vec<(MessageId, P)> {
        let message_count = message_list_header::read(reader);

        let mut last_read_id: Option<MessageId> = None;
        let mut output = Vec::new();

        for _x in 0..message_count {
            let id_w_msg = Self::read_incoming_message(reader, converter, &last_read_id);
            last_read_id = Some(id_w_msg.0);
            output.push(id_w_msg);
        }
        return output;
    }

    fn read_incoming_message(
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
        last_read_id: &Option<MessageId>,
    ) -> (MessageId, P) {
        let message_id: MessageId;
        if let Some(last_id) = last_read_id {
            let id_diff = UnsignedVariableInteger::<3>::de(reader).unwrap().get() as MessageId;
            message_id = last_id.wrapping_add(id_diff);
        } else {
            // read message id
            message_id = MessageId::de(reader).unwrap();
        }

        // read payload
        let new_message = P::build(reader, converter);

        return (message_id, new_message);
    }
}
