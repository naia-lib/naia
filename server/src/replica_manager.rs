use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use slotmap::SparseSecondaryMap;

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    DiffMask, EntityKey, KeyGenerator, LocalEntityKey, LocalComponentKey, Manifest, NaiaKey,
    PacketNotifiable, ProtocolType, Ref, Replicate, MTU_SIZE, ComponentRecord
};

use crate::packet_writer::PacketWriter;

use super::{
    entity_record::EntityRecord,
    keys::component_key::ComponentKey,
    locality_status::LocalityStatus,
    mut_handler::MutHandler,
    replica_action::ReplicaAction,
    replica_record::ReplicaRecord,
};

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
#[derive(Debug)]
pub struct ReplicaManager<T: ProtocolType> {
    address: SocketAddr,
    // replicas
    replica_key_generator: KeyGenerator<LocalComponentKey>,
    local_replica_store: SparseSecondaryMap<ComponentKey, Ref<dyn Replicate<T>>>,
    local_to_global_replica_key_map: HashMap<LocalComponentKey, ComponentKey>,
    replica_records: SparseSecondaryMap<ComponentKey, ReplicaRecord>,
    delayed_replica_deletions: HashSet<ComponentKey>,
    // entities
    entity_key_generator: KeyGenerator<LocalEntityKey>,
    local_entity_store: HashMap<EntityKey, EntityRecord>,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, EntityKey>,
    pawn_entity_store: HashSet<EntityKey>,
    delayed_entity_deletions: HashSet<EntityKey>,
    // messages / updates / ect
    queued_messages: VecDeque<ReplicaAction<T>>,
    sent_messages: HashMap<u16, Vec<ReplicaAction<T>>>,
    sent_updates: HashMap<u16, HashMap<ComponentKey, Ref<DiffMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler>,
    last_popped_diff_mask: Option<DiffMask>,
    last_popped_diff_mask_list: Option<Vec<(ComponentKey, DiffMask)>>,
}

impl<T: ProtocolType> ReplicaManager<T> {
    /// Create a new ReplicaManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler>) -> Self {
        ReplicaManager {
            address,
            // replicas
            replica_key_generator: KeyGenerator::new(),
            local_replica_store: SparseSecondaryMap::new(),
            local_to_global_replica_key_map: HashMap::new(),
            replica_records: SparseSecondaryMap::new(),
            delayed_replica_deletions: HashSet::new(),
            // entities
            entity_key_generator: KeyGenerator::new(),
            local_to_global_entity_key_map: HashMap::new(),
            local_entity_store: HashMap::new(),
            pawn_entity_store: HashSet::new(),
            delayed_entity_deletions: HashSet::new(),
            // messages / updates / ect
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ComponentKey, Ref<DiffMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_diff_mask: None,
            last_popped_diff_mask_list: None,
        }
    }

    pub fn has_outgoing_actions(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_action(&mut self, packet_index: u16) -> Option<ReplicaAction<T>> {
        let queued_message_opt = self.queued_messages.pop_front();
        if queued_message_opt.is_none() {
            return None;
        }
        let mut message = queued_message_opt.unwrap();

        let replacement_message: Option<ReplicaAction<T>> = {
            match &message {
                ReplicaAction::CreateEntity(global_entity_key, local_entity_key, _) => {
                    let mut component_list = Vec::new();

                    let entity_record = self.local_entity_store.get(global_entity_key)
                        .expect("trying to pop an replica action for an entity which has not been initialized correctly");

                    for global_component_key in entity_record.get_component_keys() {
                        let component_ref = self.local_replica_store.get(global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        let component_record = self.replica_records.get(global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        component_list.push((
                            global_component_key,
                            component_record.local_key,
                            component_ref.clone(),
                        ));
                    }

                    Some(ReplicaAction::CreateEntity(
                        *global_entity_key,
                        *local_entity_key,
                        Some(component_list),
                    ))
                }
                _ => None,
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

        //clear diff mask of replica if need be
        match &message {
            ReplicaAction::AddComponent(_, global_key, _, _) => {
                self.pop_create_replica_diff_mask(global_key);
            }
            ReplicaAction::CreateEntity(_, _, components_list_opt) => {
                if let Some(components_list) = components_list_opt {
                    let mut diff_mask_list: Vec<(ComponentKey, DiffMask)> = Vec::new();
                    for (global_component_key, _, _) in components_list {
                        if let Some(record) = self.replica_records.get(*global_component_key) {
                            diff_mask_list.push((
                                *global_component_key,
                                record.get_diff_mask().borrow().clone(),
                            ));
                        }
                        self.mut_handler
                            .borrow_mut()
                            .clear_replica(&self.address, global_component_key);
                    }
                    self.last_popped_diff_mask_list = Some(diff_mask_list);
                }
            }
            ReplicaAction::UpdateReplica(global_key, local_key, diff_mask, replica) => {
                return Some(self.pop_update_replica_diff_mask(
                    packet_index,
                    global_key,
                    local_key,
                    diff_mask,
                    replica,
                ));
            }
            _ => {}
        }

        return Some(message);
    }

    pub fn unpop_outgoing_action(&mut self, packet_index: u16, message: &ReplicaAction<T>) {
        info!("unpopping");
        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.pop();
            if sent_messages_list.len() == 0 {
                self.sent_messages.remove(&packet_index);
            }
        }

        match &message {
            ReplicaAction::AddComponent(_, global_key, _, _) => {
                self.unpop_create_replica_diff_mask(global_key);
            }
            ReplicaAction::CreateEntity(_, _, _) => {
                if let Some(last_popped_diff_mask_list) = &self.last_popped_diff_mask_list {
                    for (global_component_key, last_popped_diff_mask) in last_popped_diff_mask_list
                    {
                        self.mut_handler.borrow_mut().set_replica(
                            &self.address,
                            global_component_key,
                            &last_popped_diff_mask,
                        );
                    }
                }
            }
            ReplicaAction::UpdateReplica(global_key, local_key, _, replica) => {
                let cloned_message = self.unpop_update_replica_diff_mask(
                    packet_index,
                    global_key,
                    local_key,
                    replica,
                );
                self.queued_messages.push_front(cloned_message);
                return;
            }
            _ => {}
        }

        self.queued_messages.push_front(message.clone());
    }

    // Entities

    pub fn add_entity(
        &mut self,
        global_key: &EntityKey,
        component_record_ref: &Ref<ComponentRecord<ComponentKey>>,
        component_list: &Vec<(ComponentKey, Ref<dyn Replicate<T>>)>,
    ) {
        if !self.local_entity_store.contains_key(global_key) {
            // first, add components
            for (component_key, component_ref) in component_list {
                self.replica_init(component_key, component_ref, LocalityStatus::Creating);
            }

            // then, add entity
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map
                .insert(local_key, *global_key);
            let entity_record = EntityRecord::new(local_key, component_record_ref);
            self.local_entity_store.insert(*global_key, entity_record);
            self.queued_messages.push_back(ReplicaAction::CreateEntity(
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

                    // Entity deletion IS Component deletion, so update those replica records
                    // accordingly
                    for component_key in entity_record.get_component_keys() {
                        if let Some(replica_record) = self.replica_records.get_mut(component_key) {
                            replica_record.status = LocalityStatus::Deleting;
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
                let local_key = self.local_entity_store.get(key).unwrap().local_key;
                self.queued_messages
                    .push_back(ReplicaAction::AssignPawnEntity(*key, local_key));
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
            let local_key = self
                .local_entity_store
                .get(key)
                .expect(
                    "expecting an entity record to exist if that entity is designated as a pawn",
                )
                .local_key;

            self.queued_messages
                .push_back(ReplicaAction::UnassignPawnEntity(*key, local_key));
        } else {
            panic!("attempting to unassign an entity as a pawn which is not assigned as a pawn in the first place")
        }
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.pawn_entity_store.contains(key);
    }

    // Components

    // Called when the entity already exists in this connection
    pub fn add_component(
        &mut self,
        entity_key: &EntityKey,
        component_key: &ComponentKey,
        component_ref: &Ref<dyn Replicate<T>>,
    ) {
        if !self.local_entity_store.contains_key(entity_key) {
            panic!(
                "attempting to add component to entity that does not yet exist for this connection"
            );
        }

        let local_component_key =
            self.replica_init(component_key, component_ref, LocalityStatus::Creating);

        let entity_record = self.local_entity_store.get(entity_key).unwrap();

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated components will be created after entity is created
            }
            LocalityStatus::Created => {
                // send add component message
                self.queued_messages.push_back(ReplicaAction::AddComponent(
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

    pub fn remove_component(&mut self, key: &ComponentKey) {
        if let Some(replica_record) = self.replica_records.get_mut(*key) {
            match replica_record.status {
                LocalityStatus::Creating => {
                    // queue deletion message to be sent after creation
                    self.delayed_replica_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion message
                    replica_delete(&mut self.queued_messages, replica_record, key);
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        } else {
            panic!(
                "attempting to remove a replica from a connection within which it does not exist"
            );
        }
    }

    // Ect..

    pub fn get_global_entity_key_from_local(
        &self,
        local_key: LocalEntityKey,
    ) -> Option<&EntityKey> {
        return self.local_to_global_entity_key_map.get(&local_key);
    }

    pub fn collect_replica_updates(&mut self) {
        for (key, record) in self.replica_records.iter() {
            if record.status == LocalityStatus::Created
                && !record.get_diff_mask().borrow().is_clear()
            {
                if let Some(replica_ref) = self.local_replica_store.get(key) {
                    self.queued_messages.push_back(ReplicaAction::UpdateReplica(
                        key,
                        record.local_key,
                        record.get_diff_mask().clone(),
                        replica_ref.clone(),
                    ));
                }
            }
        }
    }

    pub fn write_replica_action(
        &self,
        packet_writer: &mut PacketWriter,
        manifest: &Manifest<T>,
        message: &ReplicaAction<T>,
    ) -> bool {
        let mut replica_total_bytes = Vec::<u8>::new();

        //Write replica message type
        replica_total_bytes
            .write_u8(message.as_type().to_u8())
            .unwrap(); // write replica message type

        match message {
            ReplicaAction::DeleteReplica(_, local_key) => {
                replica_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ReplicaAction::UpdateReplica(_, local_key, diff_mask, replica) => {
                //write replica payload
                let mut replica_payload_bytes = Vec::<u8>::new();
                replica
                    .borrow()
                    .write_partial(&diff_mask.borrow(), &mut replica_payload_bytes);

                //Write replica "header"
                replica_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                diff_mask.borrow_mut().write(&mut replica_total_bytes); // write diff mask
                replica_total_bytes.append(&mut replica_payload_bytes); // write
                                                                        // payload
            }
            ReplicaAction::CreateEntity(_, local_entity_key, component_list_opt) => {
                replica_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key

                // get list of components
                if let Some(component_list) = component_list_opt {
                    let components_num = component_list.len();
                    if components_num > 255 {
                        panic!("no entity should have so many components... fix this");
                    }
                    replica_total_bytes.write_u8(components_num as u8).unwrap(); //write number of components

                    for (_, local_component_key, component_ref) in component_list {
                        //write component payload
                        let mut component_payload_bytes = Vec::<u8>::new();
                        component_ref.borrow().write(&mut component_payload_bytes);

                        //Write component "header"
                        let type_id = component_ref.borrow().get_type_id();
                        let naia_id = manifest.get_naia_id(&type_id); // get naia id
                        replica_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                        replica_total_bytes
                            .write_u16::<BigEndian>(local_component_key.to_u16())
                            .unwrap(); //write local key
                        replica_total_bytes.append(&mut component_payload_bytes);
                        // write payload
                    }
                } else {
                    replica_total_bytes.write_u8(0).unwrap();
                }
            }
            ReplicaAction::DeleteEntity(_, local_key) => {
                replica_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ReplicaAction::AssignPawnEntity(_, local_key) => {
                replica_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ReplicaAction::UnassignPawnEntity(_, local_key) => {
                replica_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ReplicaAction::AddComponent(local_entity_key, _, local_component_key, component) => {
                //write component payload
                let mut component_payload_bytes = Vec::<u8>::new();
                component.borrow().write(&mut component_payload_bytes);

                //Write component "header"
                replica_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key
                let type_id = component.borrow().get_type_id();
                let naia_id = manifest.get_naia_id(&type_id); // get naia id
                replica_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                replica_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                replica_total_bytes.append(&mut component_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + replica_total_bytes.len();
        if packet_writer.replica_action_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if packet_writer.replica_action_count == 255 {
                return false;
            }
            packet_writer.replica_action_count = packet_writer.replica_action_count.wrapping_add(1);
            packet_writer
                .replica_working_bytes
                .append(&mut replica_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    // Private methods

    fn replica_init(
        &mut self,
        key: &ComponentKey,
        replica: &Ref<dyn Replicate<T>>,
        status: LocalityStatus,
    ) -> LocalComponentKey {
        if !self.local_replica_store.contains_key(*key) {
            self.local_replica_store.insert(*key, replica.clone());
            let local_key: LocalComponentKey = self.replica_key_generator.generate();
            self.local_to_global_replica_key_map.insert(local_key, *key);
            let diff_mask_size = replica.borrow().get_diff_mask_size();
            let replica_record = ReplicaRecord::new(local_key, diff_mask_size, status);
            self.mut_handler.borrow_mut().register_mask(
                &self.address,
                &key,
                replica_record.get_diff_mask(),
            );
            self.replica_records.insert(*key, replica_record);
            return local_key;
        } else {
            // Should panic, as this is not dependent on any unreliable transport factor
            panic!("attempted to add replica twice..");
        }
    }

    fn replica_cleanup(&mut self, global_component_key: &ComponentKey) {
        if let Some(replica_record) = self.replica_records.remove(*global_component_key) {
            // actually delete the replica from local records
            let local_component_key = replica_record.local_key;
            self.mut_handler
                .borrow_mut()
                .deregister_mask(&self.address, global_component_key);
            self.local_replica_store.remove(*global_component_key);
            self.local_to_global_replica_key_map
                .remove(&local_component_key);
            self.replica_key_generator.recycle_key(&local_component_key);
        } else {
            // likely due to duplicate delivered deletion messages
            warn!("attempting to clean up replica from connection inside which it is not present");
        }
    }

    fn pop_create_replica_diff_mask(&mut self, global_key: &ComponentKey) {
        if let Some(record) = self.replica_records.get(*global_key) {
            self.last_popped_diff_mask = Some(record.get_diff_mask().borrow().clone());
        }
        self.mut_handler
            .borrow_mut()
            .clear_replica(&self.address, global_key);
    }

    fn unpop_create_replica_diff_mask(&mut self, global_key: &ComponentKey) {
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_replica(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }
    }

    fn pop_update_replica_diff_mask(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        local_key: &LocalComponentKey,
        diff_mask: &Ref<DiffMask>,
        replica: &Ref<dyn Replicate<T>>,
    ) -> ReplicaAction<T> {
        let locked_diff_mask = self.process_replica_update(packet_index, global_key, diff_mask);
        // return new Update message to be written
        return ReplicaAction::UpdateReplica(
            *global_key,
            *local_key,
            locked_diff_mask,
            replica.clone(),
        );
    }

    fn unpop_update_replica_diff_mask(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        local_key: &LocalComponentKey,
        replica: &Ref<dyn Replicate<T>>,
    ) -> ReplicaAction<T> {
        let original_diff_mask = self.undo_replica_update(&packet_index, &global_key);

        return ReplicaAction::UpdateReplica(
            *global_key,
            *local_key,
            original_diff_mask,
            replica.clone(),
        );
    }

    fn process_replica_update(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        diff_mask: &Ref<DiffMask>,
    ) -> Ref<DiffMask> {
        // previously the diff mask was the CURRENT diff mask for the
        // replica, we want to lock that in so we know exactly what we're
        // writing
        let locked_diff_mask = Ref::new(diff_mask.borrow().clone());

        // place diff mask in a special transmission record - like map
        if !self.sent_updates.contains_key(&packet_index) {
            let sent_updates_map: HashMap<ComponentKey, Ref<DiffMask>> = HashMap::new();
            self.sent_updates.insert(packet_index, sent_updates_map);
            self.last_last_update_packet_index = self.last_update_packet_index;
            self.last_update_packet_index = packet_index;
        }

        if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
            sent_updates_map.insert(*global_key, locked_diff_mask.clone());
        }

        // having copied the diff mask for this update, clear the replica
        self.last_popped_diff_mask = Some(diff_mask.borrow().clone());
        self.mut_handler
            .borrow_mut()
            .clear_replica(&self.address, global_key);

        locked_diff_mask
    }

    fn undo_replica_update(&mut self, packet_index: &u16, global_key: &ComponentKey) -> Ref<DiffMask> {
        if let Some(sent_updates_map) = self.sent_updates.get_mut(packet_index) {
            sent_updates_map.remove(global_key);
            if sent_updates_map.len() == 0 {
                self.sent_updates.remove(&packet_index);
            }
        }

        self.last_update_packet_index = self.last_last_update_packet_index;
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_replica(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }

        self.replica_records
            .get(*global_key)
            .expect("uh oh, we don't have enough info to unpop the message")
            .get_diff_mask()
            .clone()
    }
}

impl<T: ProtocolType> PacketNotifiable for ReplicaManager<T> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        let mut deleted_replicas: Vec<ComponentKey> = Vec::new();

        if let Some(delivered_messages_list) = self.sent_messages.remove(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {
                match delivered_message {
                    ReplicaAction::DeleteReplica(global_component_key, _) => {
                        deleted_replicas.push(global_component_key);
                    }
                    ReplicaAction::UpdateReplica(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                    ReplicaAction::CreateEntity(global_entity_key, _, component_list_opt) => {
                        let entity_record = self.local_entity_store.get_mut(&global_entity_key)
                            .expect("created entity does not have a entity_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_entity_deletions.remove(&global_entity_key) {
                            entity_delete(
                                &mut self.queued_messages,
                                entity_record,
                                &global_entity_key,
                            );
                        } else {
                            // set to status of created
                            entity_record.status = LocalityStatus::Created;

                            // set status of components to created
                            if let Some(mut component_list) = component_list_opt {
                                while let Some((global_component_key, _, _)) = component_list.pop()
                                {
                                    let component_record = self
                                        .replica_records
                                        .get_mut(global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }
                            }

                            // for any components on this entity that have not yet been created
                            // initiate that now
                            for component_key in entity_record.get_component_keys() {
                                let component_record = self
                                    .replica_records
                                    .get(component_key)
                                    .expect("component not created correctly?");
                                // check if component has been successfully created
                                // (perhaps through the previous entity_create operation)
                                if component_record.status == LocalityStatus::Creating {
                                    let component_ref = self
                                        .local_replica_store
                                        .get(component_key)
                                        .expect("component not created correctly?");
                                    self.queued_messages.push_back(ReplicaAction::AddComponent(
                                        entity_record.local_key,
                                        component_key,
                                        component_record.local_key,
                                        component_ref.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    ReplicaAction::DeleteEntity(global_key, local_key) => {
                        let entity_record = self
                            .local_entity_store
                            .remove(&global_key)
                            .expect("deletion of nonexistent entity!");

                        // actually delete the entity from local records
                        self.local_to_global_entity_key_map.remove(&local_key);
                        self.entity_key_generator.recycle_key(&local_key);
                        self.pawn_entity_store.remove(&global_key);

                        // delete all associated component replicas
                        for component_key in entity_record.get_component_keys() {
                            deleted_replicas.push(component_key);
                        }
                    }
                    ReplicaAction::AssignPawnEntity(_, _) => {}
                    ReplicaAction::UnassignPawnEntity(_, _) => {}
                    ReplicaAction::AddComponent(_, global_component_key, _, _) => {
                        let component_record =
                            self.replica_records.get_mut(global_component_key).expect(
                                "added component does not have a record .. initiation problem?",
                            );
                        // do we need to delete this now?
                        if self.delayed_replica_deletions.remove(&global_component_key) {
                            replica_delete(
                                &mut self.queued_messages,
                                component_record,
                                &global_component_key,
                            );
                        } else {
                            // we do not need to delete just yet
                            component_record.status = LocalityStatus::Created;
                        }
                    }
                }
            }
        }

        for deleted_component_key in deleted_replicas {
            self.replica_cleanup(&deleted_component_key);
        }
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&dropped_packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                match dropped_message {
                    // guaranteed delivery messages
                    | ReplicaAction::DeleteReplica(_, _)
                    | ReplicaAction::CreateEntity(_, _, _)
                    | ReplicaAction::DeleteEntity(_, _)
                    | ReplicaAction::AssignPawnEntity(_, _)
                    | ReplicaAction::UnassignPawnEntity(_, _)
                    | ReplicaAction::AddComponent(_, _, _, _) => {
                        self.queued_messages.push_back(dropped_message.clone());
                    }
                    // non-guaranteed delivery messages
                    ReplicaAction::UpdateReplica(global_key, _, _, _) => {
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
                                            if let Some(diff_mask) = diff_mask_map.get(global_key) {
                                                new_diff_mask.nand(diff_mask.borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                if let Some(record) = self.replica_records.get_mut(*global_key) {
                                    let mut current_diff_mask = record.get_diff_mask().borrow_mut();
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

fn replica_delete<T: ProtocolType>(
    queued_messages: &mut VecDeque<ReplicaAction<T>>,
    record: &mut ReplicaRecord,
    component_key: &ComponentKey,
) {
    record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ReplicaAction::DeleteReplica(*component_key, record.local_key));
}

fn entity_delete<T: ProtocolType>(
    queued_messages: &mut VecDeque<ReplicaAction<T>>,
    entity_record: &mut EntityRecord,
    entity_key: &EntityKey,
) {
    entity_record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ReplicaAction::DeleteEntity(
        *entity_key,
        entity_record.local_key,
    ));
}
