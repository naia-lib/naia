use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde, UnsignedVariableInteger};

use crate::{
    messages::message_kinds::MessageKinds, types::MessageIndex, wrapping_diff, Message, ProtocolIo,
};

// Sender
pub struct IndexedMessageWriter;

impl IndexedMessageWriter {
    pub fn write_messages(
        message_kinds: &MessageKinds,
        outgoing_messages: &mut VecDeque<(MessageIndex, Box<dyn Message>)>,
        channel_writer: &ProtocolIo,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        let mut last_written_id: Option<MessageIndex> = None;
        let mut message_indexs = Vec::new();

        loop {
            if outgoing_messages.is_empty() {
                break;
            }

            // check that we can write the next message
            let (message_index, message) = outgoing_messages.front().unwrap();
            let mut counter = bit_writer.counter();
            Self::write_message(
                message_kinds,
                channel_writer,
                &mut counter,
                &last_written_id,
                message_index,
                message,
            );

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    Self::warn_overflow(counter.bits_needed(), bit_writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(bit_writer);

            // write data
            Self::write_message(
                message_kinds,
                channel_writer,
                bit_writer,
                &last_written_id,
                message_index,
                message,
            );

            message_indexs.push(*message_index);
            last_written_id = Some(*message_index);

            // pop message we've written
            outgoing_messages.pop_front();
        }
        Some(message_indexs)
    }

    fn write_message(
        message_kinds: &MessageKinds,
        channel_writer: &ProtocolIo,
        bit_writer: &mut dyn BitWrite,
        last_written_id: &Option<MessageIndex>,
        message_index: &MessageIndex,
        message: &Box<dyn Message>,
    ) {
        if let Some(last_id) = last_written_id {
            // write message id diff
            let id_diff = wrapping_diff(*last_id, *message_index);
            let id_diff_encoded = UnsignedVariableInteger::<3>::new(id_diff);
            id_diff_encoded.ser(bit_writer);
        } else {
            // write message id
            message_index.ser(bit_writer);
        }

        channel_writer.write(message_kinds, bit_writer, message);
    }

    fn warn_overflow(bits_needed: u16, bits_free: u16) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Message requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
        )
    }
}
