use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use slotmap::SparseSecondaryMap;

use naia_shared::{State, StateNotifiable, ProtocolType, KeyGenerator, LocalObjectKey, Ref, DiffMask, EntityKey, LocalEntityKey};

use super::{
    object_key::{object_key::ObjectKey, ComponentKey},
    object_record::ObjectRecord,
    locality_status::LocalityStatus,
    entity_record::EntityRecord,
    mut_handler::MutHandler,
    server_state_message::ServerStateMessage,
};

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{Manifest, MTU_SIZE, NaiaKey};

use crate::server_packet_writer::ServerPacketWriter;

/// Manages States/Entities for a given Client connection and keeps them in sync on the
/// Client
#[derive(Debug)]
pub struct ServerStateManager<T: ProtocolType> {
    address: SocketAddr,
    // objects
    object_key_generator: KeyGenerator<LocalObjectKey>,
    local_object_store: SparseSecondaryMap<ObjectKey, Ref<dyn State<T>>>,
    local_to_global_object_key_map: HashMap<LocalObjectKey, ObjectKey>,
    object_records: SparseSecondaryMap<ObjectKey, ObjectRecord>,
    pawn_object_store: HashSet<ObjectKey>,
    delayed_object_deletions: HashSet<ObjectKey>,
    // entities
    entity_key_generator: KeyGenerator<LocalEntityKey>,
    local_entity_store: HashMap<EntityKey, EntityRecord>,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, EntityKey>,
    pawn_entity_store: HashSet<EntityKey>,
    delayed_entity_deletions: HashSet<EntityKey>,
    // messages / updates / ect
    queued_messages: VecDeque<ServerStateMessage<T>>,
    sent_messages: HashMap<u16, Vec<ServerStateMessage<T>>>,
    sent_updates: HashMap<u16, HashMap<ObjectKey, Ref<DiffMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler>,
    last_popped_diff_mask: Option<DiffMask>,
    last_popped_diff_mask_list: Option<Vec<(ObjectKey, DiffMask)>>,
}

impl<T: ProtocolType> ServerStateManager<T> {
    /// Create a new ServerStateManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler>) -> Self {
        ServerStateManager {
            address,
            // states
            object_key_generator: KeyGenerator::new(),
            local_object_store: SparseSecondaryMap::new(),
            local_to_global_object_key_map: HashMap::new(),
            object_records: SparseSecondaryMap::new(),
            pawn_object_store: HashSet::new(),
            delayed_object_deletions: HashSet::new(),
            // entities
            entity_key_generator: KeyGenerator::new(),
            local_to_global_entity_key_map: HashMap::new(),
            local_entity_store: HashMap::new(),
            pawn_entity_store: HashSet::new(),
            delayed_entity_deletions: HashSet::new(),
            // messages / updates / ect
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ObjectKey, Ref<DiffMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_diff_mask: None,
            last_popped_diff_mask_list: None,
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<ServerStateMessage<T>> {
        let queued_message_opt = self.queued_messages.pop_front();
        if queued_message_opt.is_none() {
            return None;
        }
        let mut message = queued_message_opt.unwrap();

        let replacement_message: Option<ServerStateMessage<T>> = {
            match &message {
                ServerStateMessage::CreateEntity(global_entity_key, local_entity_key, _) => {
                    let mut component_list = Vec::new();

                    let entity_record = self.local_entity_store.get(global_entity_key)
                        .expect("trying to pop an state message for an entity which has not been initialized correctly");

                    let components: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                    for global_component_key in components {
                        let component_ref = self.local_object_store.get(*global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        let component_record = self.object_records.get(*global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        component_list.push((*global_component_key, component_record.local_key, component_ref.clone()));
                    }

                    Some(ServerStateMessage::CreateEntity(*global_entity_key, *local_entity_key, Some(component_list)))
                }
                _ => None
            }
        };

        if let Some(new_message) = replacement_message {
            message = new_message;
        }

        if !self.sent_messages.contains_key(&packet_index) {
            self.sent_messages.insert(packet_index, Vec::new());
        }

        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.push(message.clone());
        }

        //clear state mask of state if need be
        match &message {
            ServerStateMessage::CreateState(global_key, _, _) => {
                self.pop_create_state_diff_mask(global_key);
            }
            ServerStateMessage::AddComponent(_, global_key, _, _) => {
                self.pop_create_state_diff_mask(global_key);
            }
            ServerStateMessage::CreateEntity(_, _, components_list_opt) => {
                if let Some(components_list) = components_list_opt {
                    let mut diff_mask_list: Vec<(ComponentKey, DiffMask)> = Vec::new();
                    for (global_component_key, _, _) in components_list {
                        if let Some(record) = self.object_records.get(*global_component_key) {
                            diff_mask_list.push((*global_component_key, record.get_diff_mask().borrow().clone()));
                        }
                        self.mut_handler
                            .borrow_mut()
                            .clear_state(&self.address, global_component_key);
                    }
                    self.last_popped_diff_mask_list = Some(diff_mask_list);
                }
            }
            ServerStateMessage::UpdateState(global_key, local_key, diff_mask, state) => {
                return Some(self.pop_update_state_diff_mask(false, packet_index, global_key, local_key, diff_mask, state));
            }
            ServerStateMessage::UpdatePawn(global_key, local_key, diff_mask, state) => {
                return Some(self.pop_update_state_diff_mask(true, packet_index, global_key, local_key, diff_mask, state));
            }
            _ => {}
        }

        return Some(message);
    }

    pub fn unpop_outgoing_message(&mut self, packet_index: u16, message: &ServerStateMessage<T>) {
        info!("unpopping");
        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.pop();
            if sent_messages_list.len() == 0 {
                self.sent_messages.remove(&packet_index);
            }
        }

        match &message {
            ServerStateMessage::CreateState(global_key, _, _) => {
                self.unpop_create_state_diff_mask(global_key);
            }
            ServerStateMessage::AddComponent(_, global_key, _, _) => {
                self.unpop_create_state_diff_mask(global_key);
            }
            ServerStateMessage::CreateEntity(_, _, _) => {
                if let Some(last_popped_diff_mask_list) = &self.last_popped_diff_mask_list {
                    for (global_component_key, last_popped_diff_mask) in last_popped_diff_mask_list {
                        self.mut_handler.borrow_mut().set_state(
                            &self.address,
                            global_component_key,
                            &last_popped_diff_mask,
                        );
                    }
                }
            }
            ServerStateMessage::UpdateState(global_key, local_key, _, state) => {
                let cloned_message = self.unpop_update_state_diff_mask(false, packet_index, global_key, local_key, state);
                self.queued_messages.push_front(cloned_message);
                return;
            }
            ServerStateMessage::UpdatePawn(global_key, local_key, _, state) => {
                let cloned_message = self.unpop_update_state_diff_mask(true, packet_index, global_key, local_key, state);
                self.queued_messages.push_front(cloned_message);
                return;
            }
            _ => {}
        }

        self.queued_messages.push_front(message.clone());
    }

    // States

    pub fn add_state(&mut self, key: &ObjectKey, state: &Ref<dyn State<T>>) {
        let local_key = self.state_init(key, state, LocalityStatus::Creating);

        self.queued_messages
            .push_back(ServerStateMessage::CreateState(
                *key,
                local_key,
                state.clone(),
            ));
    }

    pub fn remove_state(&mut self, key: &ObjectKey) {

        if self.has_pawn(key) {
            self.remove_pawn(key);
        }

        if let Some(object_record) = self.object_records.get_mut(*key) {
            match object_record.status {
                LocalityStatus::Creating => {
                    // queue deletion message to be sent after creation
                    self.delayed_object_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion message
                    state_delete(&mut self.queued_messages, object_record, key);
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        } else {
            panic!("attempting to remove an state from a connection within which it does not exist");
        }
    }

    pub fn has_object(&self, key: &ObjectKey) -> bool {
        return self.local_object_store.contains_key(*key);
    }

    // Pawns

    pub fn add_pawn(&mut self, key: &ObjectKey) {
        if self.local_object_store.contains_key(*key) {
            if !self.pawn_object_store.contains(key) {
                self.pawn_object_store.insert(*key);
                if let Some(object_record) = self.object_records.get_mut(*key) {
                    self.queued_messages
                        .push_back(ServerStateMessage::AssignPawn(*key, object_record.local_key));
                }
            }
        } else {
            panic!("user connection does not have local state to make into a pawn!");
        }
    }

    pub fn remove_pawn(&mut self, key: &ObjectKey) {
        if self.pawn_object_store.remove(key) {
            if let Some(object_record) = self.object_records.get_mut(*key) {
                self.queued_messages
                    .push_back(ServerStateMessage::UnassignPawn(
                        *key,
                        object_record.local_key,
                    ));
            }
        } else {
            panic!("attempt to unassign a pawn state from a connection to which it is not assigned as a pawn in the first place")
        }
    }

    pub fn has_pawn(&self, key: &ObjectKey) -> bool {
        return self.pawn_object_store.contains(key);
    }

    // Entities

    pub fn add_entity(&mut self, global_key: &EntityKey,
                      components_ref: &Ref<HashSet<ComponentKey>>,
                      component_list: &Vec<(ComponentKey, Ref<dyn State<T>>)>) {
        if !self.local_entity_store.contains_key(global_key) {
            // first, add components
            for (component_key, component_ref) in component_list {
                self.state_init(component_key, component_ref, LocalityStatus::Creating);
            }

            // then, add entity
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map.insert(local_key, *global_key);
            let entity_record = EntityRecord::new(local_key, components_ref);
            self.local_entity_store.insert(*global_key, entity_record);
            self.queued_messages
                .push_back(ServerStateMessage::CreateEntity(
                    *global_key,
                    local_key,
                    None,
                ));
        } else {
            panic!("added entity twice");
        }
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        if self.has_pawn_entity(key) {
            self.remove_pawn_entity(key);
        }

        if let Some(entity_record) = self.local_entity_store.get_mut(key) {

            match entity_record.status {
                LocalityStatus::Creating => {
                    // queue deletion message to be sent after creation
                    self.delayed_entity_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion message
                    entity_delete(&mut self.queued_messages, entity_record, key);

                    // Entity deletion IS Component deletion, so update those state records accordingly
                    let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                    for component_key in component_set {
                        self.pawn_object_store.remove(component_key);

                        if let Some(object_record) = self.object_records.get_mut(*component_key) {
                            object_record.status = LocalityStatus::Deleting;
                        }
                    }
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        }
    }

    pub fn has_entity(&self, key: &EntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    // Pawn Entities

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        if self.local_entity_store.contains_key(key) {
            if !self.pawn_entity_store.contains(key) {
                self.pawn_entity_store.insert(*key);
                let local_key = self.local_entity_store.get(key)
                    .unwrap()
                    .local_key;
                self.queued_messages
                    .push_back(ServerStateMessage::AssignPawnEntity(*key, local_key));
            } else {
                warn!("attempting to assign a pawn entity twice");
            }
        } else {
            warn!("attempting to assign a nonexistent entity to be a pawn");
        }
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        if self.pawn_entity_store.contains(key) {
            self.pawn_entity_store.remove(key);
            let local_key = self.local_entity_store.get(key)
                .expect("expecting an entity record to exist if that entity is designated as a pawn")
                .local_key;

            self.queued_messages
                .push_back(ServerStateMessage::UnassignPawnEntity(
                    *key,
                    local_key,
                ));
        } else {
            panic!("attempting to unassign an entity as a pawn which is not assigned as a pawn in the first place")
        }
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.pawn_entity_store.contains(key);
    }

    // Components

    // called when the entity already exists in this connection
    pub fn add_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey, component_ref: &Ref<dyn State<T>>) {
        if !self.local_entity_store.contains_key(entity_key) {
            panic!("attempting to add component to entity that does not yet exist for this connection");
        }

        let local_component_key = self.state_init(component_key, component_ref, LocalityStatus::Creating);

        let entity_record = self.local_entity_store.get(entity_key).unwrap();

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated components will be created after entity is created
            }
            LocalityStatus::Created => {
                // send add component message
                self.queued_messages
                    .push_back(ServerStateMessage::AddComponent(
                        entity_record.local_key,
                        *component_key,
                        local_component_key,
                        component_ref.clone(),
                    ));
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    // Ect..

    pub fn get_global_key_from_local(&self, local_key: LocalObjectKey) -> Option<&ObjectKey> {
        return self.local_to_global_object_key_map.get(&local_key);
    }

    pub fn get_global_entity_key_from_local(&self, local_key: LocalEntityKey) -> Option<&EntityKey> {
        return self.local_to_global_entity_key_map.get(&local_key);
    }

    pub fn collect_state_updates(&mut self) {
        for (key, record) in self.object_records.iter() {
            if record.status == LocalityStatus::Created
                && !record.get_diff_mask().borrow().is_clear()
            {
                if let Some(state_ref) = self.local_object_store.get(key) {
                    if self.pawn_object_store.contains(&key) {
                        // handle as a pawn
                        self.queued_messages
                            .push_back(ServerStateMessage::UpdatePawn(
                                key,
                                record.local_key,
                                record.get_diff_mask().clone(),
                                state_ref.clone(),
                            ));
                    } else {
                        // handle as an state
                        self.queued_messages
                            .push_back(ServerStateMessage::UpdateState(
                                key,
                                record.local_key,
                                record.get_diff_mask().clone(),
                                state_ref.clone(),
                            ));
                    }
                }
            }
        }
    }

    pub fn write_state_message(
        &self,
        packet_writer: &mut ServerPacketWriter,
        manifest: &Manifest<T>,
        message: &ServerStateMessage<T>,
    ) -> bool {
        let mut state_total_bytes = Vec::<u8>::new();

        //Write state message type
        state_total_bytes
                    .write_u8(message.as_type().to_u8())
                    .unwrap(); // write state message type

        match message {
            ServerStateMessage::CreateState(_, local_key, state) => {
                //write state payload
                let mut state_payload_bytes = Vec::<u8>::new();
                state.borrow().write(&mut state_payload_bytes);

                //Write state "header"
                let type_id = state.borrow().get_type_id();
                let naia_id = manifest.get_naia_id(&type_id); // get naia id
                state_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                state_total_bytes.append(&mut state_payload_bytes); // write payload
            }
            ServerStateMessage::DeleteState(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::UpdateState(_, local_key, diff_mask, state) => {
                //write state payload
                let mut state_payload_bytes = Vec::<u8>::new();
                state
                    .borrow()
                    .write_partial(&diff_mask.borrow(), &mut state_payload_bytes);

                //Write state "header"
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                diff_mask.borrow_mut().write(&mut state_total_bytes); // write state mask
                state_total_bytes.append(&mut state_payload_bytes); // write payload
            }
            ServerStateMessage::AssignPawn(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::UnassignPawn(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::UpdatePawn(_, local_key, _, state) => {
                //write state payload
                let mut state_payload_bytes = Vec::<u8>::new();
                state.borrow().write(&mut state_payload_bytes);

                //Write state "header"
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                state_total_bytes.append(&mut state_payload_bytes); // write payload
            }
            ServerStateMessage::CreateEntity(_, local_entity_key, component_list_opt) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key

                // get list of components
                if let Some(component_list) = component_list_opt {

                    let components_num = component_list.len();
                    if components_num > 255 {
                        panic!("no entity should have so many components... fix this");
                    }
                    state_total_bytes
                        .write_u8(components_num as u8)
                        .unwrap(); //write number of components

                    for (_, local_component_key, component_ref) in component_list {
                        //write component payload
                        let mut component_payload_bytes = Vec::<u8>::new();
                        component_ref.borrow().write(&mut component_payload_bytes);

                        //Write component "header"
                        let type_id = component_ref.borrow().get_type_id();
                        let naia_id = manifest.get_naia_id(&type_id); // get naia id
                        state_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                        state_total_bytes
                            .write_u16::<BigEndian>(local_component_key.to_u16())
                            .unwrap(); //write local key
                        state_total_bytes.append(&mut component_payload_bytes); // write payload
                    }
                } else {
                    state_total_bytes
                        .write_u8(0)
                        .unwrap();
                }
            }
            ServerStateMessage::DeleteEntity(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::AssignPawnEntity(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::UnassignPawnEntity(_, local_key) => {
                state_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerStateMessage::AddComponent(local_entity_key, _, local_component_key, component) => {
                //write component payload
                let mut component_payload_bytes = Vec::<u8>::new();
                component.borrow().write(&mut component_payload_bytes);

                //Write component "header"
                state_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key
                let type_id = component.borrow().get_type_id();
                let naia_id = manifest.get_naia_id(&type_id); // get naia id
                state_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                state_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                state_total_bytes.append(&mut component_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + state_total_bytes.len();
        if packet_writer.state_message_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if packet_writer.state_message_count == 255 {
                return false;
            }
            packet_writer.state_message_count = packet_writer.state_message_count.wrapping_add(1);
            packet_writer
                .state_working_bytes
                .append(&mut state_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    // Private methods

    fn state_init(&mut self, key: &ObjectKey, state: &Ref<dyn State<T>>, status: LocalityStatus) -> LocalObjectKey {
        if !self.local_object_store.contains_key(*key) {
            self.local_object_store.insert(*key, state.clone());
            let local_key: LocalObjectKey = self.object_key_generator.generate();
            self.local_to_global_object_key_map.insert(local_key, *key);
            let diff_mask_size = state.borrow().get_diff_mask_size();
            let object_record = ObjectRecord::new(local_key, diff_mask_size, status);
            self.mut_handler.borrow_mut().register_mask(
                &self.address,
                &key,
                object_record.get_diff_mask(),
            );
            self.object_records.insert(*key, object_record);
            return local_key;
        } else {
            // Should panic, as this is not dependent on any unreliable transport fstate
            panic!("attempted to add state twice..");
        }
    }

    fn state_cleanup(&mut self, global_object_key: &ObjectKey) {
        if let Some(object_record) = self.object_records.remove(*global_object_key) {
            // actually delete the state from local records
            let local_object_key = object_record.local_key;
            self.mut_handler
                .borrow_mut()
                .deregister_mask(&self.address, global_object_key);
            self.local_object_store.remove(*global_object_key);
            self.local_to_global_object_key_map.remove(&local_object_key);
            self.object_key_generator.recycle_key(&local_object_key);
            self.pawn_object_store.remove(&global_object_key);
        } else {
            // likely due to duplicate delivered deletion messages
            warn!("attempting to clean up state from connection inside which it is not present");
        }
    }

    fn pop_create_state_diff_mask(&mut self, global_key: &ObjectKey) {
        if let Some(record) = self.object_records.get(*global_key) {
            self.last_popped_diff_mask = Some(record.get_diff_mask().borrow().clone());
        }
        self.mut_handler
            .borrow_mut()
            .clear_state(&self.address, global_key);
    }

    fn unpop_create_state_diff_mask(&mut self, global_key: &ObjectKey) {
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_state(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }
    }

    fn pop_update_state_diff_mask(&mut self,
                                   is_pawn: bool,
                                   packet_index: u16,
                                   global_key: &ObjectKey,
                                   local_key: &LocalObjectKey,
                                   diff_mask: &Ref<DiffMask>,
                                   state: &Ref<dyn State<T>>) -> ServerStateMessage<T> {
        let locked_diff_mask =
            self.process_state_update(packet_index, global_key, diff_mask);
        // return new Update message to be written
        if is_pawn {
            return ServerStateMessage::UpdatePawn(
                *global_key,
                *local_key,
                locked_diff_mask,
                state.clone(),
            );
        } else {
            return ServerStateMessage::UpdateState(
                *global_key,
                *local_key,
                locked_diff_mask,
                state.clone(),
            );
        }
    }

    fn unpop_update_state_diff_mask(&mut self,
                                   is_pawn: bool,
                                   packet_index: u16,
                                   global_key: &ObjectKey,
                                   local_key: &LocalObjectKey,
                                   state: &Ref<dyn State<T>>) -> ServerStateMessage<T> {
        let original_diff_mask = self.undo_state_update(&packet_index, &global_key);
        if is_pawn {
            return ServerStateMessage::UpdatePawn(
                *global_key,
                *local_key,
                original_diff_mask,
                state.clone(),
            );
        } else {
            return ServerStateMessage::UpdateState(
                *global_key,
                *local_key,
                original_diff_mask,
                state.clone(),
            );
        }
    }

    fn process_state_update(
        &mut self,
        packet_index: u16,
        global_key: &ObjectKey,
        diff_mask: &Ref<DiffMask>,
    ) -> Ref<DiffMask> {
        // previously the state mask was the CURRENT state mask for the state,
        // we want to lock that in so we know exactly what we're writing
        let locked_diff_mask = Ref::new(diff_mask.borrow().clone());

        // place state mask in a special transmission record - like map
        if !self.sent_updates.contains_key(&packet_index) {
            let sent_updates_map: HashMap<ObjectKey, Ref<DiffMask>> = HashMap::new();
            self.sent_updates.insert(packet_index, sent_updates_map);
            self.last_last_update_packet_index = self.last_update_packet_index;
            self.last_update_packet_index = packet_index;
        }

        if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
            sent_updates_map.insert(*global_key, locked_diff_mask.clone());
        }

        // having copied the state mask for this update, clear the state
        self.last_popped_diff_mask = Some(diff_mask.borrow().clone());
        self.mut_handler
            .borrow_mut()
            .clear_state(&self.address, global_key);

        locked_diff_mask
    }

    fn undo_state_update(&mut self, packet_index: &u16, global_key: &ObjectKey) -> Ref<DiffMask> {
        if let Some(sent_updates_map) = self.sent_updates.get_mut(packet_index) {
            sent_updates_map.remove(global_key);
            if sent_updates_map.len() == 0 {
                self.sent_updates.remove(&packet_index);
            }
        }

        self.last_update_packet_index = self.last_last_update_packet_index;
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_state(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }

        self.object_records
            .get(*global_key)
            .expect("uh oh, we don't have enough info to unpop the message")
            .get_diff_mask()
            .clone()
    }
}

impl<T: ProtocolType> StateNotifiable for ServerStateManager<T> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        let mut deleted_states: Vec<ObjectKey> = Vec::new();

        if let Some(delivered_messages_list) = self.sent_messages.remove(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {
                match delivered_message {
                    ServerStateMessage::CreateState(global_key, _, _) => {
                        let object_record = self.object_records.get_mut(global_key)
                            .expect("created state does not have an object_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_object_deletions.remove(&global_key) {
                            state_delete(&mut self.queued_messages, object_record, &global_key);
                        } else {
                            // we do not need to delete just yet
                            object_record.status = LocalityStatus::Created;
                        }
                    }
                    ServerStateMessage::DeleteState(global_object_key, _) => {
                        deleted_states.push(global_object_key);
                    }
                    ServerStateMessage::UpdateState(_, _, _, _)
                    | ServerStateMessage::UpdatePawn(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                    ServerStateMessage::AssignPawn(_, _) => {}
                    ServerStateMessage::UnassignPawn(_, _) => {}
                    ServerStateMessage::CreateEntity(global_entity_key, _, component_list_opt) => {
                        let entity_record = self.local_entity_store.get_mut(&global_entity_key)
                            .expect("created entity does not have a entity_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_entity_deletions.remove(&global_entity_key) {
                            entity_delete(&mut self.queued_messages, entity_record, &global_entity_key);
                        } else {
                            // set to status of created
                            entity_record.status = LocalityStatus::Created;

                            // set status of components to created
                            if let Some(mut component_list) = component_list_opt {
                                while let Some((global_component_key, _, _)) = component_list.pop() {
                                    let component_record = self.object_records.get_mut(global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }
                            }

                            // for any components on this entity that have not yet been created
                            // initiate that now
                            let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                            for component_key in component_set {
                                let component_record = self.object_records.get(*component_key)
                                    .expect("component not created correctly?");
                                // check if component has been successfully created
                                // (perhaps through the previous entity_create operation)
                                if component_record.status == LocalityStatus::Creating {
                                    let component_ref = self.local_object_store.get(*component_key)
                                        .expect("component not created correctly?");
                                    self.queued_messages
                                        .push_back(ServerStateMessage::AddComponent(
                                            entity_record.local_key,
                                            *component_key,
                                            component_record.local_key,
                                            component_ref.clone(),
                                        ));
                                }
                            }
                        }
                    }
                    ServerStateMessage::DeleteEntity(global_key, local_key) => {
                        let entity_record = self.local_entity_store.remove(&global_key).expect("deletion of nonexistent entity!");

                        // actually delete the entity from local records
                        self.local_to_global_entity_key_map.remove(&local_key);
                        self.entity_key_generator.recycle_key(&local_key);
                        self.pawn_entity_store.remove(&global_key);

                        // delete all associated component states
                        let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                        for component_key in component_set {
                            deleted_states.push(*component_key);
                        }
                    }
                    ServerStateMessage::AssignPawnEntity(_, _) => {}
                    ServerStateMessage::UnassignPawnEntity(_, _) => {}
                    ServerStateMessage::AddComponent(_, global_component_key, _, _) => {
                        let component_record = self.object_records.get_mut(global_component_key)
                            .expect("added component does not have a record .. initiation problem?");
                        // do we need to delete this now?
                        if self.delayed_object_deletions.remove(&global_component_key) {
                            state_delete(&mut self.queued_messages, component_record, &global_component_key);
                        } else {
                            // we do not need to delete just yet
                            component_record.status = LocalityStatus::Created;
                        }
                    }
                }
            }
        }

        for deleted_object_key in deleted_states {
            self.state_cleanup(&deleted_object_key);
        }
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&dropped_packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                match dropped_message {
                    // gauranteed delivery messages
                    ServerStateMessage::CreateState(_, _, _)
                    | ServerStateMessage::DeleteState(_, _)
                    | ServerStateMessage::AssignPawn(_, _)
                    | ServerStateMessage::UnassignPawn(_, _)
                    | ServerStateMessage::CreateEntity(_, _, _)
                    | ServerStateMessage::DeleteEntity(_, _)
                    | ServerStateMessage::AssignPawnEntity(_, _)
                    | ServerStateMessage::UnassignPawnEntity(_, _)
                    | ServerStateMessage::AddComponent(_, _, _, _) => {
                        self.queued_messages.push_back(dropped_message.clone());
                    }
                    // non-gauranteed delivery messages
                    ServerStateMessage::UpdateState(global_key, _, _, _)
                    | ServerStateMessage::UpdatePawn(global_key, _, _, _) => {
                        if let Some(diff_mask_map) = self.sent_updates.get(&dropped_packet_index) {
                            if let Some(diff_mask) = diff_mask_map.get(global_key) {
                                let mut new_diff_mask = diff_mask.borrow().clone();

                                // walk from dropped packet up to most recently sent packet
                                if dropped_packet_index != self.last_update_packet_index {
                                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                                    while packet_index != self.last_update_packet_index {
                                        if let Some(diff_mask_map) =
                                            self.sent_updates.get(&packet_index)
                                        {
                                            if let Some(diff_mask) = diff_mask_map.get(global_key)
                                            {
                                                new_diff_mask.nand(diff_mask.borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                if let Some(record) = self.object_records.get_mut(*global_key) {
                                    let mut current_diff_mask =
                                        record.get_diff_mask().borrow_mut();
                                    current_diff_mask.or(new_diff_mask.borrow());
                                }
                            }
                        }
                    }
                }
            }

            self.sent_updates.remove(&dropped_packet_index);
            self.sent_messages.remove(&dropped_packet_index);
        }
    }
}

fn state_delete<T: ProtocolType>(queued_messages: &mut VecDeque<ServerStateMessage<T>>,
                              object_record: &mut ObjectRecord,
                              object_key: &ObjectKey) {
    object_record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ServerStateMessage::DeleteState(
            *object_key,
            object_record.local_key,
        ));
}

fn entity_delete<T: ProtocolType>(queued_messages: &mut VecDeque<ServerStateMessage<T>>,
                              entity_record: &mut EntityRecord,
                              entity_key: &EntityKey) {
    entity_record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ServerStateMessage::DeleteEntity(
        *entity_key,
        entity_record.local_key,
    ));
}