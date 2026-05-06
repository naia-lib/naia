use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde, UnsignedVariableInteger};

use crate::{
    messages::{message_container::MessageContainer, message_kinds::MessageKinds},
    types::MessageIndex,
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
    wrapping_diff,
};

// Sender
pub struct IndexedMessageWriter;

impl IndexedMessageWriter {
    pub fn write_messages(
        message_kinds: &MessageKinds,
        outgoing_messages: &mut VecDeque<(MessageIndex, MessageContainer)>,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        let mut last_written_id: Option<MessageIndex> = None;
        let mut message_indices = Vec::new();

        loop {
            if outgoing_messages.is_empty() {
                break;
            }

            let (message_index, message) = outgoing_messages.front().unwrap();

            // check that we can write the next message
            let mut counter = writer.counter();
            // write MessageContinue bit
            true.ser(&mut counter);
            // write data
            Self::write_message(
                message_kinds,
                converter,
                &mut counter,
                &last_written_id,
                message_index,
                message,
            );
            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    Self::warn_overflow(counter.bits_needed(), writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(writer);
            // write data
            Self::write_message(
                message_kinds,
                converter,
                writer,
                &last_written_id,
                message_index,
                message,
            );

            message_indices.push(*message_index);
            last_written_id = Some(*message_index);

            // pop message we've written
            outgoing_messages.pop_front();
        }
        Some(message_indices)
    }

    pub fn write_message_index(
        writer: &mut dyn BitWrite,
        last_written_id: &Option<MessageIndex>,
        message_index: &MessageIndex,
    ) {
        if let Some(last_id) = last_written_id {
            // write message id diff
            let id_diff = wrapping_diff(*last_id, *message_index);
            if id_diff < 0 {
                panic!("Packet Write Error: Message Index diff is negative in subsequent message.. Previous: {}, Current: {}, Diff: {}", last_id, message_index, id_diff);
            }
            let id_diff_encoded = UnsignedVariableInteger::<3>::new(id_diff);
            id_diff_encoded.ser(writer);
        } else {
            // write message id
            message_index.ser(writer);
        }
    }

    fn write_message(
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut dyn BitWrite,
        last_written_id: &Option<MessageIndex>,
        message_index: &MessageIndex,
        message: &MessageContainer,
    ) {
        Self::write_message_index(writer, last_written_id, message_index);

        message.write(message_kinds, writer, converter);
    }

    fn warn_overflow(bits_needed: u32, bits_free: u32) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Message requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
        )
    }
}

#[cfg(test)]
mod tests {
    //! V.2 preservation — CommandId / MessageIndex stream monotonicity.
    //!
    //! The unified priority sort may reorder BETWEEN streams (channels, entity
    //! bundles), but within a single stream (per-channel FIFO, per-entity
    //! command sequence) the delta-encoded writer requires that each id be
    //! >= the previous. If this invariant ever breaks, `write_message_index`
    //! > panics immediately — pinning the sender's monotonicity contract.
    use naia_serde::BitWriter;

    use super::IndexedMessageWriter;

    #[test]
    fn monotonic_stream_succeeds() {
        let mut writer = BitWriter::new();
        let mut last = None;
        for id in [1u16, 2, 3, 7, 100, 101, 200] {
            IndexedMessageWriter::write_message_index(&mut writer, &last, &id);
            last = Some(id);
        }
    }

    #[test]
    fn equal_ids_do_not_panic() {
        // Edge case: diff-of-zero is non-negative and permitted.
        let mut writer = BitWriter::new();
        let mut last = None;
        for id in [42u16, 42, 42] {
            IndexedMessageWriter::write_message_index(&mut writer, &last, &id);
            last = Some(id);
        }
    }

    #[test]
    #[should_panic(expected = "Message Index diff is negative")]
    fn out_of_order_within_stream_panics() {
        // Emitting 5 after 10 violates monotonicity and must panic — V.2
        // contract. If this test starts passing silently, the reorder-safety
        // of the unified priority sort is compromised.
        let mut writer = BitWriter::new();
        let mut last = None;
        for id in [10u16, 5u16] {
            IndexedMessageWriter::write_message_index(&mut writer, &last, &id);
            last = Some(id);
        }
    }
}
