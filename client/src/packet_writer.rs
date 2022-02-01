use std::hash::Hash;

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    ManagerType, MessagePacketWriter, NaiaKey, ProtocolKindType, Protocolize,
    MTU_SIZE,
};

use super::entity_manager::EntityManager;

/// Handles writing of Message/Command data into an outgoing packet
pub struct PacketWriter {
    command_working_bytes: Vec<u8>,
    command_count: u8,
    message_writer: MessagePacketWriter,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            command_working_bytes: Vec::<u8>::new(),
            command_count: 0,
            message_writer: MessagePacketWriter::new(),
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.command_count != 0 || self.message_writer.has_bytes();
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn get_bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        //Write manager "header" (manager type & command count)
        if self.command_count != 0 {
            out_bytes.write_u8(ManagerType::EntityMessage as u8).unwrap(); // write manager type
            out_bytes.write_u8(self.command_count).unwrap(); // write number of commands in the following message
            out_bytes.append(&mut self.command_working_bytes); // write command payload
            self.command_count = 0;
        }

        self.message_writer.get_bytes(&mut out_bytes);

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.command_working_bytes.len() + self.message_writer.bytes_number();
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_entity_message<P: Protocolize, E: Copy + Eq + Hash>(
        &mut self,
        entity_manager: &EntityManager<P, E>,
        world_entity: &E,
        message: &P,
    ) -> bool {
        if let Some(local_entity) = entity_manager.world_to_local_entity(&world_entity) {

            //Write command "header"
            let mut command_total_bytes = Vec::<u8>::new();

            // write local entity
            command_total_bytes
                .write_u16::<BigEndian>(local_entity.to_u16())
                .unwrap();

            // write command kind
            let command_kind = message.dyn_ref().get_kind();
            command_total_bytes
                .write_u16::<BigEndian>(command_kind.to_u16())
                .unwrap();

            // write payload
            message.dyn_ref().write(&mut command_total_bytes);

            let mut hypothetical_next_payload_size =
                self.bytes_number() + command_total_bytes.len();
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
        return true;
    }

    /// Writes a Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<P: Protocolize>(&mut self, message: &P) -> bool {
        return self.message_writer.write_message(message);
    }
}