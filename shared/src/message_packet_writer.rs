use byteorder::{BigEndian, WriteBytesExt};

use super::{
    manager_type::ManagerType, manifest::Manifest, protocol_type::ProtocolType,
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
    pub fn get_bytes(&mut self, out_bytes: &mut Vec<u8>) {
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
    pub fn write_message<P: ProtocolType>(
        &mut self,
        manifest: &Manifest<P>,
        message: P,
    ) -> bool {
        let message_ref = message.dyn_ref();

        //Write message payload
        let mut message_payload_bytes = Vec::<u8>::new();
        message_ref.write(&mut message_payload_bytes);

        //Write message "header"
        let mut message_total_bytes = Vec::<u8>::new();

        let message_kind = message_ref.get_kind();
        let naia_id = manifest.get_naia_id(&message_kind); // get naia id
        message_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
        message_total_bytes.append(&mut message_payload_bytes); // write payload

        let mut hypothetical_next_payload_size = self.bytes_number() + message_total_bytes.len();
        if self.message_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if self.message_count == 255 {
                return false;
            }
            self.message_count = self.message_count.wrapping_add(1);
            self.message_working_bytes.append(&mut message_total_bytes);
            return true;
        } else {
            return false;
        }
    }
}
