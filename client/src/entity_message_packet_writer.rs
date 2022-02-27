use std::{collections::HashMap, hash::Hash};

use crate::entity_record::EntityRecord;
use naia_shared::{
    ManagerType, NaiaKey, PacketWriteState, ProtocolKindType, Protocolize, MTU_SIZE,
};

pub struct EntityMessagePacketWriter {
    queued_bytes: Vec<u8>,
    queue_count: u8,
}

impl EntityMessagePacketWriter {
    pub fn new() -> Self {
        EntityMessagePacketWriter {
            queued_bytes: Vec::<u8>::new(),
            queue_count: 0,
        }
    }

    /// Check if Command can fit into outgoing buffer
    pub fn message_fits<P: Protocolize, E: Copy + Eq + Hash>(
        &self,
        write_state: &mut PacketWriteState,
        entity_records: &HashMap<E, EntityRecord<P::Kind>>,
        world_entity: &E,
        message: &P,
    ) -> bool {
        let mut hypothetical_next_payload_size: usize = write_state.byte_count();

        if entity_records.get(world_entity).is_some() {
            // write client tick
            hypothetical_next_payload_size += 2;

            // write local entity
            hypothetical_next_payload_size += 2;

            // write message kind
            hypothetical_next_payload_size += 2;

            // write payload
            hypothetical_next_payload_size += message.dyn_ref().kind().size();

            if self.queue_count == 0 {
                hypothetical_next_payload_size += 2;
            }
        } else {
            panic!("Cannot find the entity record to serialize entity message!");
        }

        hypothetical_next_payload_size < MTU_SIZE && self.queue_count != 255
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn queue_write<P: Protocolize, E: Copy + Eq + Hash>(
        &mut self,
        write_state: &mut PacketWriteState,
        entity_records: &HashMap<E, EntityRecord<P::Kind>>,
        client_tick: &u16,
        world_entity: &E,
        message: &P,
    ) {
        if let Some(entity_record) = entity_records.get(world_entity) {
            let message_ref = message.dyn_ref();

            let mut byte_buffer = Vec::<u8>::new();

            // write client tick
            byte_buffer.write_u16::<BigEndian>(*client_tick).unwrap();

            // write local entity
            byte_buffer
                .write_u16::<BigEndian>(entity_record.entity_net_id.to_u16())
                .unwrap();

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
        } else {
            panic!("Cannot find the entity record to serialize entity message!");
        }
    }

    /// Write bytes into an outgoing packet
    pub fn flush_writes(&mut self, out_bytes: &mut Vec<u8>) {
        if self.queue_count == 0 {
            panic!("Should not call this method if self.queue_count is 0");
        }

        //Write manager "header" (manager type & message count)

        // write manager type
        out_bytes
            .write_u8(ManagerType::EntityMessage as u8)
            .unwrap();

        // write number of messages
        out_bytes.write_u8(self.queue_count).unwrap();

        // write payload
        out_bytes.append(&mut self.queued_bytes);

        self.queue_count = 0;
        self.queued_bytes = Vec::<u8>::new();
    }
}
