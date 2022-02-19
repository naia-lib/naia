use std::{collections::HashMap, hash::Hash};

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    ManagerType, NaiaKey, PacketWriteState, ProtocolKindType, Protocolize, WorldRefType, MTU_SIZE,
};

use crate::{
    entity_action::EntityAction, local_component_record::LocalComponentRecord,
    local_entity_record::LocalEntityRecord, ComponentKey,
};

pub struct EntityActionPacketWriter {
    queued_bytes: Vec<u8>,
    queue_count: u8,
}

impl EntityActionPacketWriter {
    pub fn new() -> Self {
        EntityActionPacketWriter {
            queued_bytes: Vec::<u8>::new(),
            queue_count: 0,
        }
    }

    /// Returns whether or not the given message will fit in the outgoing buffer
    pub fn action_fits<P: Protocolize, E: Copy + Eq + Hash>(
        &self,
        write_state: &mut PacketWriteState,
        action: &EntityAction<P, E>,
    ) -> bool {
        let mut hypothetical_next_payload_size: usize = write_state.byte_count();

        //Write EntityAction type
        hypothetical_next_payload_size += 1;

        match action {
            EntityAction::SpawnEntity(_, component_list) => {
                //write local entity
                hypothetical_next_payload_size += 2;

                //write number of components
                hypothetical_next_payload_size += 1;

                for (_, component_kind) in component_list {
                    //write component payload
                    hypothetical_next_payload_size += component_kind.size();

                    //Write component "header"
                    hypothetical_next_payload_size += 2;
                    hypothetical_next_payload_size += 2;
                }
            }
            EntityAction::DespawnEntity(_) => {
                hypothetical_next_payload_size += 2;
            }
            EntityAction::MessageEntity(_, message) => {
                //write local entity
                hypothetical_next_payload_size += 2;

                // write message's naia id
                hypothetical_next_payload_size += 2;

                //Write message payload
                hypothetical_next_payload_size += message.dyn_ref().kind().size();
            }
            EntityAction::InsertComponent(_, _, component_kind) => {
                //write component payload
                hypothetical_next_payload_size += component_kind.size();

                //Write component "header"
                hypothetical_next_payload_size += 2;
                hypothetical_next_payload_size += 2;
                hypothetical_next_payload_size += 2;
            }
            EntityAction::UpdateComponent(_, _, diff_mask, component_kind) => {
                hypothetical_next_payload_size += component_kind.size_partial(diff_mask);

                //Write component "header"
                //write local component key
                hypothetical_next_payload_size += 2;
                // write diff mask
                hypothetical_next_payload_size += diff_mask.size();
            }
            EntityAction::RemoveComponent(_) => {
                hypothetical_next_payload_size += 2;
            }
        }

        if self.queue_count == 0 {
            hypothetical_next_payload_size += 2;
        }

        hypothetical_next_payload_size < MTU_SIZE && self.queue_count != 255
    }

    pub fn queue_write<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>>(
        &mut self,
        write_state: &mut PacketWriteState,
        world: &W,
        entity_records: &HashMap<E, LocalEntityRecord>,
        component_records: &HashMap<ComponentKey, LocalComponentRecord>,
        action: &EntityAction<P, E>,
    ) {
        let mut byte_buffer = Vec::<u8>::new();

        //Write EntityAction type
        byte_buffer.write_u8(action.as_type().to_u8()).unwrap();

        match action {
            EntityAction::SpawnEntity(global_entity, component_list) => {
                let local_id = entity_records.get(global_entity).unwrap().entity_net_id;

                byte_buffer
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity

                // get list of components
                let components_num = component_list.len();
                if components_num > 255 {
                    panic!("no entity should have so many components... fix this");
                }
                byte_buffer.write_u8(components_num as u8).unwrap(); //write number of components

                for (global_component_key, component_kind) in component_list {
                    let local_component_key = component_records
                        .get(global_component_key)
                        .unwrap()
                        .local_key;

                    //write component payload
                    let component_ref = world
                        .component_of_kind(global_entity, component_kind)
                        .expect("Component does not exist in World");
                    let mut component_payload_bytes = Vec::<u8>::new();
                    component_ref.write(&mut component_payload_bytes);

                    //Write component "header"
                    byte_buffer
                        .write_u16::<BigEndian>(component_kind.to_u16())
                        .unwrap(); // write naia id
                    byte_buffer
                        .write_u16::<BigEndian>(local_component_key.to_u16())
                        .unwrap(); //write local component key
                    byte_buffer.append(&mut component_payload_bytes);
                    // write payload
                }
            }
            EntityAction::DespawnEntity(global_entity) => {
                let local_id = entity_records.get(global_entity).unwrap().entity_net_id;
                byte_buffer
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity
            }
            EntityAction::MessageEntity(global_entity, message) => {
                let local_id = entity_records.get(global_entity).unwrap().entity_net_id;
                let message_ref = message.dyn_ref();

                //write local entity
                byte_buffer
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap();

                // write message's naia id
                let message_kind = message_ref.kind();
                byte_buffer
                    .write_u16::<BigEndian>(message_kind.to_u16())
                    .unwrap();

                //Write message payload
                message_ref.write(&mut byte_buffer);
            }
            EntityAction::InsertComponent(global_entity, global_component_key, component_kind) => {
                let local_id = entity_records.get(global_entity).unwrap().entity_net_id;
                let local_component_key = component_records
                    .get(global_component_key)
                    .unwrap()
                    .local_key;

                //write component payload
                let component_ref = world
                    .component_of_kind(global_entity, component_kind)
                    .expect("Component does not exist in World");

                let mut component_payload_bytes = Vec::<u8>::new();
                component_ref.write(&mut component_payload_bytes);

                //Write component "header"
                byte_buffer
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity
                byte_buffer
                    .write_u16::<BigEndian>(component_kind.to_u16())
                    .unwrap(); // write component kind
                byte_buffer
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                byte_buffer.append(&mut component_payload_bytes); // write payload
            }
            EntityAction::UpdateComponent(
                global_entity,
                global_component_key,
                diff_mask,
                component_kind,
            ) => {
                let local_component_key = component_records
                    .get(global_component_key)
                    .unwrap()
                    .local_key;

                //write component payload
                let component_ref = world
                    .component_of_kind(global_entity, component_kind)
                    .expect("Component does not exist in World");

                let mut component_payload_bytes = Vec::<u8>::new();
                component_ref.write_partial(diff_mask, &mut component_payload_bytes);

                //Write component "header"
                byte_buffer
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                diff_mask.write(&mut byte_buffer); // write diff mask
                byte_buffer.append(&mut component_payload_bytes); // write
                                                                  // payload
            }
            EntityAction::RemoveComponent(global_component_key) => {
                let local_component_key = component_records
                    .get(global_component_key)
                    .unwrap()
                    .local_key;

                byte_buffer
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local key
            }
        }

        write_state.add_bytes(self.queue_count == 0, 2, byte_buffer.len());
        self.queue_count += 1;
        self.queued_bytes.append(&mut byte_buffer);
    }

    /// Write bytes into an outgoing packet
    pub fn flush_writes(&mut self, out_bytes: &mut Vec<u8>) {
        //Write manager "header" (manager type & action count)
        if self.queue_count != 0 {
            // write manager type
            out_bytes.write_u8(ManagerType::Entity as u8).unwrap();

            // write number of actions
            out_bytes.write_u8(self.queue_count).unwrap();

            // write payload
            out_bytes.append(&mut self.queued_bytes);

            self.queue_count = 0;
            self.queued_bytes = Vec::<u8>::new();
        }
    }
}
