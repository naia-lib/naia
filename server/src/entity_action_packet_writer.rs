use std::{hash::Hash, collections::HashMap, };

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{ManagerType, MTU_SIZE, Protocolize, WorldRefType, ProtocolKindType, NaiaKey};

use crate::{entity_action::EntityAction, ComponentKey,
            local_component_record::LocalComponentRecord,
            local_entity_record::LocalEntityRecord};


pub struct EntityActionPacketWriter {
    entity_working_bytes: Vec<u8>,
    entity_action_count: u8,
}

impl EntityActionPacketWriter {
    pub fn new() -> Self {
        EntityActionPacketWriter {
            entity_working_bytes: Vec::<u8>::new(),
            entity_action_count: 0,
        }
    }

    /// Returns whether the writer has bytes to write into the outgoing packet
    pub fn has_bytes(&self) -> bool {
        return self.entity_action_count != 0;
    }

    /// Get the number of bytes which is ready to be written into an outgoing
    /// packet
    pub fn bytes_number(&self) -> usize {
        return self.entity_working_bytes.len();
    }

    /// Gets the bytes to write into an outgoing packet
    pub fn bytes(&mut self, out_bytes: &mut Vec<u8>) {
        //Write manager "header" (manager type & entity action count)
        if self.entity_action_count != 0 {

            // write manager type
            out_bytes.write_u8(ManagerType::Entity as u8).unwrap();

            // write number of actions
            out_bytes.write_u8(self.entity_action_count).unwrap();

            // write entity payload
            out_bytes.append(&mut self.entity_working_bytes);

            self.entity_action_count = 0;
            self.entity_working_bytes = Vec::<u8>::new();
        }
    }

    /// Returns whether or not the given message will fit in the outgoing buffer
    pub fn entity_action_fits<P: Protocolize, E: Copy + Eq + Hash>(
        &self,
        total_bytes: usize,
        action: &EntityAction<P, E>,
    ) -> bool {
        let mut action_total_bytes: usize = 0;

        //Write EntityAction type
        action_total_bytes += 1;

        match action {
            EntityAction::SpawnEntity(_, component_list) => {
                //write local entity
                action_total_bytes += 2;

                //write number of components
                action_total_bytes += 1;

                for (_, component_kind) in component_list {
                    //write component payload
                    action_total_bytes += component_kind.size();

                    //Write component "header"
                    action_total_bytes += 2;
                    action_total_bytes += 2;
                }
            }
            EntityAction::DespawnEntity(_) => {
                action_total_bytes += 2;
            }
            EntityAction::MessageEntity(_, message) => {
                //write local entity
                action_total_bytes += 2;

                // write message's naia id
                action_total_bytes += 2;

                //Write message payload
                action_total_bytes += message.dyn_ref().kind().size();
            }
            EntityAction::InsertComponent(_, _, component_kind) => {
                //write component payload
                action_total_bytes += component_kind.size();

                //Write component "header"
                action_total_bytes += 2;
                action_total_bytes += 2;
                action_total_bytes += 2;
            }
            EntityAction::UpdateComponent(_, _, diff_mask, component_kind) => {
                action_total_bytes += component_kind.size_partial(diff_mask);

                //Write component "header"
                //write local component key
                action_total_bytes += 2;
                // write diff mask
                action_total_bytes += diff_mask.size();
            }
            EntityAction::RemoveComponent(_) => {
                action_total_bytes += 2;
            }
        }

        let mut hypothetical_next_payload_size = total_bytes + action_total_bytes;
        if self.entity_action_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if self.entity_action_count == 255 {
                return false;
            }
            return true;
        } else {
            return false;
        }
    }

    pub fn write_entity_action<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        entity_records: &HashMap<E, LocalEntityRecord>,
        component_records: &HashMap<ComponentKey, LocalComponentRecord>,
        action: &EntityAction<P, E>,
    ) {
        let mut action_total_bytes = Vec::<u8>::new();

        //Write EntityAction type
        action_total_bytes
            .write_u8(action.as_type().to_u8())
            .unwrap();

        match action {
            EntityAction::SpawnEntity(global_entity, component_list) => {
                let local_id = entity_records
                    .get(global_entity)
                    .unwrap()
                    .entity_net_id;

                action_total_bytes
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity

                // get list of components
                let components_num = component_list.len();
                if components_num > 255 {
                    panic!("no entity should have so many components... fix this");
                }
                action_total_bytes.write_u8(components_num as u8).unwrap(); //write number of components

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
                    action_total_bytes
                        .write_u16::<BigEndian>(component_kind.to_u16())
                        .unwrap(); // write naia id
                    action_total_bytes
                        .write_u16::<BigEndian>(local_component_key.to_u16())
                        .unwrap(); //write local component key
                    action_total_bytes.append(&mut component_payload_bytes);
                    // write payload
                }
            }
            EntityAction::DespawnEntity(global_entity) => {
                let local_id = entity_records
                    .get(global_entity)
                    .unwrap()
                    .entity_net_id;
                action_total_bytes
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity
            }
            EntityAction::MessageEntity(global_entity, message) => {
                let local_id = entity_records
                    .get(global_entity)
                    .unwrap()
                    .entity_net_id;
                let message_ref = message.dyn_ref();

                //write local entity
                action_total_bytes
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap();

                // write message's naia id
                let message_kind = message_ref.kind();
                action_total_bytes
                    .write_u16::<BigEndian>(message_kind.to_u16())
                    .unwrap();

                //Write message payload
                message_ref.write(&mut action_total_bytes);
            }
            EntityAction::InsertComponent(global_entity, global_component_key, component_kind) => {
                let local_id = entity_records
                    .get(global_entity)
                    .unwrap()
                    .entity_net_id;
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
                action_total_bytes
                    .write_u16::<BigEndian>(local_id.to_u16())
                    .unwrap(); //write local entity
                action_total_bytes
                    .write_u16::<BigEndian>(component_kind.to_u16())
                    .unwrap(); // write component kind
                action_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                action_total_bytes.append(&mut component_payload_bytes); // write payload
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
                action_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                diff_mask.write(&mut action_total_bytes); // write diff mask
                action_total_bytes.append(&mut component_payload_bytes); // write
                // payload
            }
            EntityAction::RemoveComponent(global_component_key) => {
                let local_component_key = component_records
                    .get(global_component_key)
                    .unwrap()
                    .local_key;

                action_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local key
            }
        }

        self.entity_action_count = self.entity_action_count.wrapping_add(1);
        self.entity_working_bytes
            .append(&mut action_total_bytes);
    }
}