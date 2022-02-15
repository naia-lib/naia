use std::hash::Hash;

use naia_shared::{MessagePacketWriter, Protocolize};

use super::{
    entity_manager::EntityManager, entity_message_packet_writer::EntityMessagePacketWriter,
};

/// Handles writing of Message/EntityMessage data into an outgoing packet
pub struct PacketWriter {
    entity_message_writer: EntityMessagePacketWriter,
    message_writer: MessagePacketWriter,
}

impl PacketWriter {
    /// Construct a new instance of `PacketReader`, the given `buffer` will be
    /// used to read information from.
    pub fn new() -> PacketWriter {
        PacketWriter {
            entity_message_writer: EntityMessagePacketWriter::new(),
            message_writer: MessagePacketWriter::new(),
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.entity_message_writer.has_bytes() || self.message_writer.has_bytes();
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn bytes(&mut self) -> Box<[u8]> {
        let mut out_bytes = Vec::<u8>::new();

        self.entity_message_writer.bytes(&mut out_bytes);
        self.message_writer.bytes(&mut out_bytes);

        out_bytes.into_boxed_slice()
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.entity_message_writer.bytes_number() + self.message_writer.bytes_number();
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_entity_message<P: Protocolize, E: Copy + Eq + Hash>(
        &mut self,
        entity_manager: &EntityManager<P, E>,
        world_entity: &E,
        message: &P,
        client_tick: &u16,
    ) -> bool {
        return self.entity_message_writer.write_entity_message(
            self.bytes_number(),
            entity_manager,
            world_entity,
            message,
            client_tick,
        );
    }

    /// Writes a Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<P: Protocolize>(&mut self, message: &P) -> bool {
        return self
            .message_writer
            .write_message(self.bytes_number(), message);
    }
}
