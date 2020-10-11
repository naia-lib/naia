use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{ActorType, EventType, Manifest, MTU_SIZE};

use super::server_actor_message::ServerActorMessage;

use crate::server_packet_writer::ServerPacketWriter;

/// Writes into a packet with Actor data
#[derive(Debug)]
pub struct ActorPacketWriter {}

impl ActorPacketWriter {
    /// Given a general PacketWriter, the manifest, and a buffered
    /// ActorMessage, actually write Actor data into the packet
    pub fn write_actor_message<T: EventType, U: ActorType>(
        packet_writer: &mut ServerPacketWriter,
        manifest: &Manifest<T, U>,
        message: &ServerActorMessage<U>,
    ) -> bool {
        let mut actor_total_bytes = Vec::<u8>::new();

        match message {
            ServerActorMessage::CreateActor(_, local_key, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor.as_ref().borrow().write(&mut actor_payload_bytes);

                //Write actor "header"
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); // write actor message type

                let type_id = actor.as_ref().borrow().get_type_id();
                let naia_id = manifest.get_actor_naia_id(&type_id); // get naia id
                actor_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
            ServerActorMessage::DeleteActor(_, local_key) => {
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); //Write actor message type
                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
            }
            ServerActorMessage::UpdateActor(_, local_key, state_mask, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor
                    .as_ref()
                    .borrow()
                    .write_partial(&state_mask.as_ref().borrow(), &mut actor_payload_bytes);

                //Write actor "header"
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); // write actor message type

                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
                state_mask
                    .as_ref()
                    .borrow_mut()
                    .write(&mut actor_total_bytes); // write state mask
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
            ServerActorMessage::AssignPawn(_, local_key) => {
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); //Write actor message type
                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
            }
            ServerActorMessage::UnassignPawn(_, local_key) => {
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); //Write actor message type
                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
            }
            ServerActorMessage::UpdatePawn(_, local_key, _, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor.as_ref().borrow().write(&mut actor_payload_bytes);

                //Write actor "header"
                actor_total_bytes
                    .write_u8(message.write_message_type())
                    .unwrap(); // write actor message type

                actor_total_bytes
                    .write_u16::<BigEndian>(*local_key)
                    .unwrap(); //write local key
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + actor_total_bytes.len();
        if packet_writer.actor_message_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            packet_writer.actor_message_count += 1;
            packet_writer
                .actor_working_bytes
                .append(&mut actor_total_bytes);
            return true;
        } else {
            return false;
        }
    }
}
