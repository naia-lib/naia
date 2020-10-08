use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    EntityType, Event, EventPacketWriter, EventType, LocalEntityKey, ManagerType, Manifest,
    MTU_SIZE,
};

use super::command_receiver::CommandReceiver;

/// Handles writing of Event & Entity data into an outgoing packet
pub struct ClientPacketWriter {
    command_working_bytes: Vec<u8>,
    command_count: u8,
    event_writer: EventPacketWriter,
}

impl ClientPacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> ClientPacketWriter {
        ClientPacketWriter {
            command_working_bytes: Vec::<u8>::new(),
            command_count: 0,
            event_writer: EventPacketWriter::new(),
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.command_count != 0 || self.event_writer.has_bytes();
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        //Write manager "header" (manager type & entity count)
        if self.command_count != 0 {
            out_bytes.write_u8(ManagerType::Command as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.command_count).unwrap(); // write number of events in the following message
            out_bytes.append(&mut self.command_working_bytes); // write event payload
            self.command_count = 0;
        }

        self.event_writer.get_bytes(&mut out_bytes);

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.command_working_bytes.len() + self.event_writer.bytes_number();
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_command<T: EventType, U: EntityType>(
        &mut self,
        manifest: &Manifest<T, U>,
        _command_receiver: &CommandReceiver<T>,
        pawn_key: LocalEntityKey,
        command: &Box<dyn Event<T>>,
    ) -> bool {
        //Write command payload
        let mut command_payload_bytes = Vec::<u8>::new();
        // TODO: write multiple commands here
        command.as_ref().write(&mut command_payload_bytes);
        if command_payload_bytes.len() > 255 {
            error!("cannot encode an event with more than 255 bytes, need to implement this");
        }

        //Write event "header" (event id & payload length)
        let mut command_total_bytes = Vec::<u8>::new();

        let type_id = command.as_ref().get_type_id();
        command_total_bytes
            .write_u16::<BigEndian>(pawn_key)
            .unwrap(); // write pawn key
        let naia_id = manifest.get_event_naia_id(&type_id); // get naia id
        command_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
        command_total_bytes
            .write_u8(command_payload_bytes.len() as u8)
            .unwrap(); // write payload length
        command_total_bytes.append(&mut command_payload_bytes); // write payload

        let mut hypothetical_next_payload_size = self.bytes_number() + command_total_bytes.len();
        if self.command_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            self.command_count += 1;
            self.command_working_bytes.append(&mut command_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    /// Writes an Event into the Writer's internal buffer, which will eventually
    /// be put into the outgoing packet
    pub fn write_event<T: EventType, U: EntityType>(
        &mut self,
        manifest: &Manifest<T, U>,
        event: &Box<dyn Event<T>>,
    ) -> bool {
        return self.event_writer.write_event(manifest, event);
    }
}
