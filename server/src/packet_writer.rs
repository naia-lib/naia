use byteorder::WriteBytesExt;

use naia_shared::{ProtocolType, Replicate, MessagePacketWriter, ManagerType, Manifest};

/// Handles writing of Message/Object/Component data into an outgoing packet
pub struct PacketWriter {
    message_writer: MessagePacketWriter,
    /// bytes representing outgoing Message/Object/Component messages / updates
    pub replicate_working_bytes: Vec<u8>,
    /// number of Message/Object/Component messages to be written
    pub replicate_action_count: u8,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            message_writer: MessagePacketWriter::new(),
            replicate_working_bytes: Vec::<u8>::new(),
            replicate_action_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.message_writer.has_bytes() || self.replicate_action_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        self.message_writer.get_bytes(&mut out_bytes);

        //Write manager "header" (manager type & replicate count)
        if self.replicate_action_count != 0 {
            out_bytes.write_u8(ManagerType::Replicate as u8).unwrap(); // write
                                                                   // manager
                                                                   // type
            out_bytes.write_u8(self.replicate_action_count).unwrap(); // write number of messages
            out_bytes.append(&mut self.replicate_working_bytes); // write replicate payload

            self.replicate_action_count = 0;
        }

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.message_writer.bytes_number() + self.replicate_working_bytes.len();
    }

    /// Writes an Message into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet
    pub fn write_message<T: ProtocolType>(
        &mut self,
        manifest: &Manifest<T>,
        message: &Box<dyn Replicate<T>>,
    ) -> bool {
        return self.message_writer.write_message(manifest, message);
    }
}