use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{EntityType, EventType, Manifest, PacketWriter, MTU_SIZE};

use super::server_entity_message::ServerEntityMessage;

/// Writes into a packet with Entity data
#[derive(Debug)]
pub struct EntityPacketWriter {}

impl EntityPacketWriter {
    /// Given a general PacketWriter, the manifest, and a buffered
    /// EntityMessage, actually write Entity data into the packet.
    /// Returns true if it was able to write a message
    pub fn write_entity_message<T: EventType, U: EntityType>(
        packet_writer: &mut PacketWriter,
        manifest: &Manifest<T, U>,
        message: &ServerEntityMessage<U>,
    ) -> bool {
        let mut entity_total_bytes = Vec::<u8>::new();

        match message {
            ServerEntityMessage::Create(_, local_key, entity) => {
                //write entity payload
                let mut entity_payload_bytes = Vec::<u8>::new();
                entity.as_ref().borrow().write(&mut entity_payload_bytes);
                if entity_payload_bytes.len() > 255 {
                    error!(
                        "cannot encode an entity with more than 255 bytes, need to implement this"
                    );
                }

                //Write entity "header" (entity id & payload length)
                entity_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); // write entity message type

                let type_id = entity.as_ref().borrow().get_type_id();
                let naia_id = manifest.get_entity_naia_id(&type_id); // get naia id
                entity_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                entity_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
                entity_total_bytes
                    .write_u8(entity_payload_bytes.len() as u8)
                    .unwrap(); // write payload length
                entity_total_bytes.append(&mut entity_payload_bytes); // write payload
            }
            ServerEntityMessage::Delete(_, local_key) => {
                entity_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); //Write entity message type
                entity_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
            }
            ServerEntityMessage::Update(_, local_key, state_mask, entity) => {
                //write entity payload
                let mut entity_payload_bytes = Vec::<u8>::new();
                entity
                    .as_ref()
                    .borrow()
                    .write_partial(&state_mask.as_ref().borrow(), &mut entity_payload_bytes);
                if entity_payload_bytes.len() > 255 {
                    error!(
                        "cannot encode an entity with more than 255 bytes, need to implement this"
                    );
                }

                //Write entity "header" (entity id & payload length)
                entity_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); // write entity message type

                entity_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
                state_mask
                    .as_ref()
                    .borrow_mut()
                    .write(&mut entity_total_bytes); // write state mask
                entity_total_bytes
                    .write_u8(entity_payload_bytes.len() as u8)
                    .unwrap(); // write payload length
                entity_total_bytes.append(&mut entity_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + entity_total_bytes.len();
        if packet_writer.entity_message_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            packet_writer.entity_message_count += 1;
            packet_writer
                .entity_working_bytes
                .append(&mut entity_total_bytes);
            return true;
        } else {
            return false;
        }
    }
}
