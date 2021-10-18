use byteorder::WriteBytesExt;

use naia_shared::{ManagerType, MessagePacketWriter, ProtocolType};

/// Handles writing of Message/Component data into an outgoing packet
pub struct PacketWriter {
    message_writer: MessagePacketWriter,
    /// bytes representing outgoing Message/Component messages / updates
    pub entity_working_bytes: Vec<u8>,
    /// number of Message/Component messages to be written
    pub entity_action_count: u8,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            message_writer: MessagePacketWriter::new(),
            entity_working_bytes: Vec::<u8>::new(),
            entity_action_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.message_writer.has_bytes() || self.entity_action_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        self.message_writer.get_bytes(&mut out_bytes);

        //Write manager "header" (manager type & entity action count)
        if self.entity_action_count != 0 {
            out_bytes.write_u8(ManagerType::Entity as u8).unwrap(); // write
                                                                    // manager
                                                                    // type
            out_bytes.write_u8(self.entity_action_count).unwrap(); // write number of actions
            out_bytes.append(&mut self.entity_working_bytes); // write entity payload

            self.entity_action_count = 0;
        }

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.message_writer.bytes_number() + self.entity_working_bytes.len();
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<P: ProtocolType>(
        &mut self,
        message: &P,
    ) -> bool {
        return self.message_writer.write_message(message);
    }
}
