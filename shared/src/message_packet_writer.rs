use crate::{PacketWriteState, MTU_SIZE};

use super::{
    manager_type::ManagerType,
    protocolize::{ProtocolKindType, Protocolize},
};

/// Handles writing of Message data into an outgoing packet
pub struct MessagePacketWriter {
    queued_bytes: Vec<u8>,
    queue_count: u8,
}

impl MessagePacketWriter {
    /// Construct a new instance of `MessagePacketWriter`, the given `buffer`
    /// will be used to read information from.
    pub fn new() -> MessagePacketWriter {
        MessagePacketWriter {
            queued_bytes: Vec::<u8>::new(),
            queue_count: 0,
        }
    }

    /// Returns whether or not the given message will fit in the outgoing buffer
    pub fn message_fits<P: Protocolize>(
        &self,
        write_state: &mut PacketWriteState,
        message: &P,
    ) -> bool {
        let mut hypothetical_next_payload_size: usize = write_state.byte_count();

        // write message kind
        hypothetical_next_payload_size += 2;

        // write payload
        hypothetical_next_payload_size += message.dyn_ref().kind().size();

        if self.queue_count == 0 {
            hypothetical_next_payload_size += 2;
        }

        hypothetical_next_payload_size < MTU_SIZE && self.queue_count != 255
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn queue_write<P: Protocolize>(&mut self, write_state: &mut PacketWriteState, message: &P) {
        let message_ref = message.dyn_ref();

        let mut byte_buffer = Vec::<u8>::new();

        // write message kind
        let message_kind = message_ref.kind();
        byte_buffer
            .write_u16::<BigEndian>(message_kind.to_u16())
            .unwrap();

        // write payload
        message_ref.write(&mut byte_buffer);

        write_state.add_bytes(self.queue_count == 0, 2, byte_buffer.len());
        self.queue_count += 1;
        self.queued_bytes.append(&mut byte_buffer);
    }

    /// Write bytes into an outgoing packet
    pub fn flush_writes(&mut self, out_bytes: &mut Vec<u8>) {
        //Write manager "header" (manager type & message count)
        if self.queue_count != 0 {
            // write manager type
            out_bytes.write_u8(ManagerType::Message as u8).unwrap();

            // write number of messages
            out_bytes.write_u8(self.queue_count).unwrap();

            // write payload
            out_bytes.append(&mut self.queued_bytes);

            self.queue_count = 0;
            self.queued_bytes = Vec::<u8>::new();
        }
    }
}
