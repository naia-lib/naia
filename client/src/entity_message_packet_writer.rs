use std::collections::HashMap;
use std::hash::Hash;

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{ManagerType, NaiaKey, ProtocolKindType, Protocolize, MTU_SIZE};
use crate::entity_record::EntityRecord;

pub struct EntityMessagePacketWriter {
    message_working_bytes: Vec<u8>,
    message_count: u8,
}

impl EntityMessagePacketWriter {
    pub fn new() -> Self {
        EntityMessagePacketWriter {
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
        //Write manager "header" (manager type & command count)
        if self.message_count != 0 {
            out_bytes
                .write_u8(ManagerType::EntityMessage as u8)
                .unwrap(); // write manager type
            out_bytes.write_u8(self.message_count).unwrap(); // write number of commands in the following message
            out_bytes.append(&mut self.message_working_bytes); // write command payload
            self.message_count = 0;
        }
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.message_working_bytes.len();
    }

    /// Check if Command can fit into outgoing buffer
    pub fn entity_message_fits<P: Protocolize, E: Copy + Eq + Hash>(
        &self,
        total_bytes: usize,
        entity_records: &HashMap<E, EntityRecord<P::Kind>>,
        world_entity: &E,
        message: &P,
    ) -> bool {
        if entity_records.get(world_entity).is_some() {
            let mut message_total_bytes: usize = 0;

            // write client tick
            message_total_bytes += 2;

            // write local entity
            message_total_bytes += 2;

            // write message kind
            message_total_bytes += 2;

            // write payload
            message_total_bytes += message.dyn_ref().kind().size();

            let mut hypothetical_next_payload_size = total_bytes + message_total_bytes;
            if self.message_count == 0 {
                hypothetical_next_payload_size += 2;
            }
            return hypothetical_next_payload_size < MTU_SIZE && self.message_count != 255;
        }
        return true;
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_entity_message<P: Protocolize, E: Copy + Eq + Hash>(
        &mut self,
        entity_records: &HashMap<E, EntityRecord<P::Kind>>,
        world_entity: &E,
        message: &P,
        client_tick: &u16,
    ) {
        if let Some(entity_record) = entity_records.get(world_entity) {
            let message_ref = message.dyn_ref();

            let mut message_total_bytes = Vec::<u8>::new();

            // write client tick
            message_total_bytes
                .write_u16::<BigEndian>(*client_tick)
                .unwrap();

            // write local entity
            message_total_bytes
                .write_u16::<BigEndian>(entity_record.entity_net_id.to_u16())
                .unwrap();

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
    }
}
