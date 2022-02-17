use byteorder::{BigEndian, WriteBytesExt};

use super::{
    manager_type::ManagerType,
    protocolize::{ProtocolKindType, Protocolize},
    standard_header::StandardHeader,
};

/// The maximum of bytes that can be used for the payload of a given packet. (See #38 of http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/)
pub const MTU_SIZE: usize = 508 - StandardHeader::bytes_number();

/// Handles writing of Message data into an outgoing packet
pub struct MessagePacketWriter {
    message_working_bytes: Vec<u8>,
    message_count: u8,
}

impl MessagePacketWriter {
    /// Construct a new instance of `MessagePacketWriter`, the given `buffer`
    /// will be used to read information from.
    pub fn new() -> MessagePacketWriter {
        MessagePacketWriter {
            message_working_bytes: Vec::<u8>::new(),
            message_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.message_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn bytes(&mut self, out_bytes: &mut Vec<u8>) {
        //Write manager "header" (manager type & message count)
        if self.message_count != 0 {
            out_bytes.write_u8(ManagerType::Message as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.message_count).unwrap(); // write number of messages in the following message
            out_bytes.append(&mut self.message_working_bytes); // write message payload
            self.message_count = 0;
        }
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.message_working_bytes.len();
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<P: Protocolize>(&mut self, message: &P) {
        let message_ref = message.dyn_ref();

        let mut message_total_bytes = Vec::<u8>::new();

        // write message kind
        let message_kind = message_ref.kind();
        message_total_bytes
            .write_u16::<BigEndian>(message_kind.to_u16())
            .unwrap();

        // write payload
        message_ref.write(&mut message_total_bytes);

        self.message_count += 1;
        self.message_working_bytes.append(&mut message_total_bytes);
    }

    /// Returns whether or not the given message will fit in the outgoing buffer
    pub fn message_fits<P: Protocolize>(&mut self, total_bytes: usize, message: &P) -> bool {
        let mut message_total_bytes: usize = 0;

        // write message kind
        message_total_bytes += 2;

        // write payload
        message_total_bytes += message.dyn_ref().kind().size();

        let mut hypothetical_next_payload_size = total_bytes + message_total_bytes;
        if self.message_count == 0 {
            hypothetical_next_payload_size += 2;
        }

        hypothetical_next_payload_size < MTU_SIZE && self.message_count != 255
    }
}
