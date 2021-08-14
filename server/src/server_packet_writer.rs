use byteorder::WriteBytesExt;

use naia_shared::{ProtocolType, State, EventPacketWriter, ManagerType, Manifest};

/// Handles writing of Event & State data into an outgoing packet
pub struct ServerPacketWriter {
    event_writer: EventPacketWriter,
    /// bytes representing outgoing State messages / updates
    pub state_working_bytes: Vec<u8>,
    /// number of State messages to be written
    pub state_message_count: u8,
}

impl ServerPacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> ServerPacketWriter {
        ServerPacketWriter {
            event_writer: EventPacketWriter::new(),
            state_working_bytes: Vec::<u8>::new(),
            state_message_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.event_writer.has_bytes() || self.state_message_count != 0;
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        self.event_writer.get_bytes(&mut out_bytes);

        //Write manager "header" (manager type & state count)
        if self.state_message_count != 0 {
            out_bytes.write_u8(ManagerType::State as u8).unwrap(); // write
                                                                   // manager
                                                                   // type
            out_bytes.write_u8(self.state_message_count).unwrap(); // write number of messages
            out_bytes.append(&mut self.state_working_bytes); // write event payload

            self.state_message_count = 0;
        }

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.event_writer.bytes_number() + self.state_working_bytes.len();
    }

    /// Writes an Event into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet
    pub fn write_event<T: ProtocolType>(
        &mut self,
        manifest: &Manifest<T>,
        event: &Box<dyn State<T>>,
    ) -> bool {
        return self.event_writer.write_event(manifest, event);
    }
}
