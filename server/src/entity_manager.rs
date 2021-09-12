use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use slotmap::SparseSecondaryMap;

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    DiffMask, EntityKey, KeyGenerator, LocalComponentKey, LocalEntityKey, Manifest, NaiaKey,
    PacketNotifiable, ProtocolType, Ref, Replicate, MTU_SIZE,
};

use crate::packet_writer::PacketWriter;

use super::{
    entity_action::EntityAction, keys::component_key::ComponentKey,
    local_component_record::LocalComponentRecord, local_entity_record::LocalEntityRecord,
    locality_status::LocalityStatus, mut_handler::MutHandler,
};
use crate::entity_record::EntityRecord;

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
#[derive(Debug)]
pub struct EntityManager<P: ProtocolType> {
    address: SocketAddr,
    // Entities
    entity_key_generator: KeyGenerator<LocalEntityKey>,
    local_entity_records: HashMap<EntityKey, LocalEntityRecord>,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, EntityKey>,
    delayed_entity_deletions: HashSet<EntityKey>,
    // Components
    component_key_generator: KeyGenerator<LocalComponentKey>,
    local_component_store: SparseSecondaryMap<ComponentKey, Ref<dyn Replicate<P>>>,
    local_to_global_component_key_map: HashMap<LocalComponentKey, ComponentKey>,
    local_component_records: SparseSecondaryMap<ComponentKey, LocalComponentRecord>,
    delayed_component_deletions: HashSet<ComponentKey>,
    // Actions / updates / ect
    queued_actions: VecDeque<EntityAction<P>>,
    sent_actions: HashMap<u16, Vec<EntityAction<P>>>,
    sent_updates: HashMap<u16, HashMap<ComponentKey, Ref<DiffMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler>,
    last_popped_diff_mask: Option<DiffMask>,
    last_popped_diff_mask_list: Option<Vec<(ComponentKey, DiffMask)>>,
}

impl<P: ProtocolType> EntityManager<P> {
    /// Create a new EntityManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler>) -> Self {
        EntityManager {
            address,
            // Entities
            entity_key_generator: KeyGenerator::new(),
            local_entity_records: HashMap::new(),
            local_to_global_entity_key_map: HashMap::new(),
            delayed_entity_deletions: HashSet::new(),
            // Components
            component_key_generator: KeyGenerator::new(),
            local_component_store: SparseSecondaryMap::new(),
            local_to_global_component_key_map: HashMap::new(),
            local_component_records: SparseSecondaryMap::new(),
            delayed_component_deletions: HashSet::new(),
            // Actions / updates / ect
            queued_actions: VecDeque::new(),
            sent_actions: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ComponentKey, Ref<DiffMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_diff_mask: None,
            last_popped_diff_mask_list: None,
        }
    }

    pub fn has_outgoing_actions(&self) -> bool {
        return self.queued_actions.len() != 0;
    }

    pub fn pop_outgoing_action(&mut self, packet_index: u16) -> Option<EntityAction<P>> {
        let queued_action_opt = self.queued_actions.pop_front();
        if queued_action_opt.is_none() {
            return None;
        }
        let mut action = queued_action_opt.unwrap();

        let replacement_action: Option<EntityAction<P>> = {
            match &action {
                EntityAction::SpawnEntity(global_entity_key, local_entity_key, _) => {
                    let mut component_list = Vec::new();

                    let entity_record = self.local_entity_records.get(global_entity_key)
                        .expect("trying to pop an entity action for an entity which has not been initialized correctly");

                    for global_component_key in entity_record.get_component_keys() {
                        let component_ref = self.local_component_store.get(global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        let component_record = self.local_component_records.get(global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        component_list.push((
                            global_component_key,
                            component_record.local_key,
                            component_ref.clone(),
                        ));
                    }

                    Some(EntityAction::SpawnEntity(
                        *global_entity_key,
                        *local_entity_key,
                        Some(component_list),
                    ))
                }
                _ => None,
            }
        };

        if let Some(new_action) = replacement_action {
            action = new_action;
        }

        if !self.sent_actions.contains_key(&packet_index) {
            self.sent_actions.insert(packet_index, Vec::new());
        }

        if let Some(sent_actions_list) = self.sent_actions.get_mut(&packet_index) {
            sent_actions_list.push(action.clone());
        }

        //clear diff mask of component if need be
        match &action {
            EntityAction::InsertComponent(_, global_key, _, _) => {
                self.pop_insert_component_diff_mask(global_key);
            }
            EntityAction::SpawnEntity(_, _, components_list_opt) => {
                if let Some(components_list) = components_list_opt {
                    let mut diff_mask_list: Vec<(ComponentKey, DiffMask)> = Vec::new();
                    for (global_component_key, _, _) in components_list {
                        if let Some(record) =
                            self.local_component_records.get(*global_component_key)
                        {
                            diff_mask_list.push((
                                *global_component_key,
                                record.get_diff_mask().borrow().clone(),
                            ));
                        }
                        self.mut_handler
                            .borrow_mut()
                            .clear_component(&self.address, global_component_key);
                    }
                    self.last_popped_diff_mask_list = Some(diff_mask_list);
                }
            }
            EntityAction::UpdateComponent(global_key, local_key, diff_mask, component) => {
                return Some(self.pop_update_component_diff_mask(
                    packet_index,
                    global_key,
                    local_key,
                    diff_mask,
                    component,
                ));
            }
            _ => {}
        }

        return Some(action);
    }

    pub fn unpop_outgoing_action(&mut self, packet_index: u16, action: &EntityAction<P>) {
        info!("unpopping");
        if let Some(sent_actions_list) = self.sent_actions.get_mut(&packet_index) {
            sent_actions_list.pop();
            if sent_actions_list.len() == 0 {
                self.sent_actions.remove(&packet_index);
            }
        }

        match &action {
            EntityAction::InsertComponent(_, global_key, _, _) => {
                self.unpop_insert_component_diff_mask(global_key);
            }
            EntityAction::SpawnEntity(_, _, _) => {
                if let Some(last_popped_diff_mask_list) = &self.last_popped_diff_mask_list {
                    for (global_component_key, last_popped_diff_mask) in last_popped_diff_mask_list
                    {
                        self.mut_handler.borrow_mut().set_component(
                            &self.address,
                            global_component_key,
                            &last_popped_diff_mask,
                        );
                    }
                }
            }
            EntityAction::UpdateComponent(global_key, local_key, _, component) => {
                let cloned_action = self.unpop_update_component_diff_mask(
                    packet_index,
                    global_key,
                    local_key,
                    component,
                );
                self.queued_actions.push_front(cloned_action);
                return;
            }
            _ => {}
        }

        self.queued_actions.push_front(action.clone());
    }

    // Entities

    pub fn add_entity(
        &mut self,
        global_key: &EntityKey,
        entity_record: &EntityRecord,
        component_list: &Vec<(ComponentKey, Ref<dyn Replicate<P>>)>,
    ) {
        if !self.local_entity_records.contains_key(global_key) {
            // first, add components
            for (component_key, component_ref) in component_list {
                self.component_init(component_key, component_ref, LocalityStatus::Creating);
            }

            // then, add entity
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map
                .insert(local_key, *global_key);
            let entity_record =
                LocalEntityRecord::new(local_key, &entity_record.get_component_record());
            self.local_entity_records.insert(*global_key, entity_record);
            self.queued_actions
                .push_back(EntityAction::SpawnEntity(*global_key, local_key, None));
        } else {
            panic!("added entity twice");
        }
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        if self.has_pawn_entity(key) {
            self.remove_pawn_entity(key);
        }

        if let Some(entity_record) = self.local_entity_records.get_mut(key) {
            match entity_record.status {
                LocalityStatus::Creating => {
                    // queue deletion action to be sent after creation
                    self.delayed_entity_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion action
                    entity_delete(&mut self.queued_actions, entity_record, key);

                    // Entity deletion IS Component deletion, so update those component records
                    // accordingly
                    for component_key in entity_record.get_component_keys() {
                        if let Some(component_record) =
                            self.local_component_records.get_mut(component_key)
                        {
                            component_record.status = LocalityStatus::Deleting;
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
        return self.local_entity_records.contains_key(key);
    }

    // Pawn Entities

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        let entity_record = self
            .local_entity_records
            .get_mut(key)
            .expect("attempting to assign a nonexistent Entity");
        if entity_record.is_pawn {
            panic!("attempting to assign an Entity twice!");
        }

        // success
        entity_record.is_pawn = true;
        self.queued_actions
            .push_back(EntityAction::OwnEntity(*key, entity_record.local_key));
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        let entity_record = self
            .local_entity_records
            .get_mut(key)
            .expect("attempting to disown on Entity which is not in-scope");
        if !entity_record.is_pawn {
            panic!("attempting to disown an Entity which is not currently assigned");
        }

        // success
        entity_record.is_pawn = false;
        self.queued_actions
            .push_back(EntityAction::DisownEntity(*key, entity_record.local_key));
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        if let Some(entity_record) = self.local_entity_records.get(key) {
            return entity_record.is_pawn;
        }
        return false;
    }

    // Components

    pub fn insert_component(
        &mut self,
        entity_key: &EntityKey,
        component_key: &ComponentKey,
        component_ref: &Ref<dyn Replicate<P>>,
    ) {
        if !self.local_entity_records.contains_key(entity_key) {
            panic!(
                "attempting to add Component to Entity that does not yet exist for this connection"
            );
        }

        let local_component_key =
            self.component_init(component_key, component_ref, LocalityStatus::Creating);

        let entity_record = self.local_entity_records.get(entity_key).unwrap(); // checked this above

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated Components will be created after Entity is created
            }
            LocalityStatus::Created => {
                // send InsertComponent action
                self.queued_actions.push_back(EntityAction::InsertComponent(
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
        let component_record = self.local_component_records.get_mut(*key).expect(
            "attempting to remove a component from a connection within which it does not exist",
        );

        match component_record.status {
            LocalityStatus::Creating => {
                // queue deletion action to be sent after creation
                self.delayed_component_deletions.insert(*key);
            }
            LocalityStatus::Created => {
                // send deletion action
                component_delete(&mut self.queued_actions, component_record, key);
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    // Ect..

    pub fn get_global_entity_key_from_local(
        &self,
        local_key: LocalEntityKey,
    ) -> Option<&EntityKey> {
        return self.local_to_global_entity_key_map.get(&local_key);
    }

    pub fn collect_component_updates(&mut self) {
        for (key, record) in self.local_component_records.iter() {
            if record.status == LocalityStatus::Created
                && !record.get_diff_mask().borrow().is_clear()
            {
                if let Some(component_ref) = self.local_component_store.get(key) {
                    self.queued_actions.push_back(EntityAction::UpdateComponent(
                        key,
                        record.local_key,
                        record.get_diff_mask().clone(),
                        component_ref.clone(),
                    ));
                }
            }
        }
    }

    pub fn write_entity_action(
        &self,
        packet_writer: &mut PacketWriter,
        manifest: &Manifest<P>,
        action: &EntityAction<P>,
    ) -> bool {
        let mut action_total_bytes = Vec::<u8>::new();

        //Write EntityAction type
        action_total_bytes
            .write_u8(action.as_type().to_u8())
            .unwrap();

        match action {
            EntityAction::RemoveComponent(_, local_key) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            EntityAction::UpdateComponent(_, local_key, diff_mask, component) => {
                //write component payload
                let mut component_payload_bytes = Vec::<u8>::new();
                component
                    .borrow()
                    .write_partial(&diff_mask.borrow(), &mut component_payload_bytes);

                //Write component "header"
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                diff_mask.borrow_mut().write(&mut action_total_bytes); // write diff mask
                action_total_bytes.append(&mut component_payload_bytes); // write
                                                                         // payload
            }
            EntityAction::SpawnEntity(_, local_entity_key, component_list_opt) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key

                // get list of components
                if let Some(component_list) = component_list_opt {
                    let components_num = component_list.len();
                    if components_num > 255 {
                        panic!("no entity should have so many components... fix this");
                    }
                    action_total_bytes.write_u8(components_num as u8).unwrap(); //write number of components

                    for (_, local_component_key, component_ref) in component_list {
                        //write component payload
                        let mut component_payload_bytes = Vec::<u8>::new();
                        component_ref.borrow().write(&mut component_payload_bytes);

                        //Write component "header"
                        let type_id = component_ref.borrow().get_type_id();
                        let naia_id = manifest.get_naia_id(&type_id); // get naia id
                        action_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                        action_total_bytes
                            .write_u16::<BigEndian>(local_component_key.to_u16())
                            .unwrap(); //write local key
                        action_total_bytes.append(&mut component_payload_bytes);
                        // write payload
                    }
                } else {
                    action_total_bytes.write_u8(0).unwrap();
                }
            }
            EntityAction::DespawnEntity(_, local_key) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            EntityAction::OwnEntity(_, local_key) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            EntityAction::DisownEntity(_, local_key) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            EntityAction::InsertComponent(local_entity_key, _, local_component_key, component) => {
                //write component payload
                let mut component_payload_bytes = Vec::<u8>::new();
                component.borrow().write(&mut component_payload_bytes);

                //Write component "header"
                action_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key
                let type_id = component.borrow().get_type_id();
                let naia_id = manifest.get_naia_id(&type_id); // get naia id
                action_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                action_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                action_total_bytes.append(&mut component_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + action_total_bytes.len();
        if packet_writer.entity_action_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if packet_writer.entity_action_count == 255 {
                return false;
            }
            packet_writer.entity_action_count = packet_writer.entity_action_count.wrapping_add(1);
            packet_writer
                .entity_working_bytes
                .append(&mut action_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    // Private methods

    fn component_init(
        &mut self,
        key: &ComponentKey,
        component: &Ref<dyn Replicate<P>>,
        status: LocalityStatus,
    ) -> LocalComponentKey {
        if self.local_component_store.contains_key(*key) {
            // Should panic, as this is not dependent on any unreliable transport factor
            panic!("attempted to add component twice..");
        }

        self.local_component_store.insert(*key, component.clone());
        let local_key: LocalComponentKey = self.component_key_generator.generate();
        self.local_to_global_component_key_map
            .insert(local_key, *key);
        let diff_mask_size = component.borrow().get_diff_mask_size();
        let component_record = LocalComponentRecord::new(local_key, diff_mask_size, status);
        self.mut_handler.borrow_mut().register_mask(
            &self.address,
            &key,
            component_record.get_diff_mask(),
        );
        self.local_component_records.insert(*key, component_record);
        return local_key;
    }

    fn component_cleanup(&mut self, global_component_key: &ComponentKey) {
        if let Some(component_record) = self.local_component_records.remove(*global_component_key) {
            // actually delete the component from local records
            let local_component_key = component_record.local_key;
            self.mut_handler
                .borrow_mut()
                .deregister_mask(&self.address, global_component_key);
            self.local_component_store.remove(*global_component_key);
            self.local_to_global_component_key_map
                .remove(&local_component_key);
            self.component_key_generator
                .recycle_key(&local_component_key);
        } else {
            // likely due to duplicate delivered deletion actions
            warn!(
                "attempting to clean up component from connection inside which it is not present"
            );
        }
    }

    fn pop_insert_component_diff_mask(&mut self, global_key: &ComponentKey) {
        if let Some(record) = self.local_component_records.get(*global_key) {
            self.last_popped_diff_mask = Some(record.get_diff_mask().borrow().clone());
        }
        self.mut_handler
            .borrow_mut()
            .clear_component(&self.address, global_key);
    }

    fn unpop_insert_component_diff_mask(&mut self, global_key: &ComponentKey) {
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_component(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }
    }

    fn pop_update_component_diff_mask(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        local_key: &LocalComponentKey,
        diff_mask: &Ref<DiffMask>,
        component: &Ref<dyn Replicate<P>>,
    ) -> EntityAction<P> {
        let locked_diff_mask = self.process_component_update(packet_index, global_key, diff_mask);
        // return new Update action to be written
        return EntityAction::UpdateComponent(
            *global_key,
            *local_key,
            locked_diff_mask,
            component.clone(),
        );
    }

    fn unpop_update_component_diff_mask(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        local_key: &LocalComponentKey,
        component: &Ref<dyn Replicate<P>>,
    ) -> EntityAction<P> {
        let original_diff_mask = self.undo_component_update(&packet_index, &global_key);

        return EntityAction::UpdateComponent(
            *global_key,
            *local_key,
            original_diff_mask,
            component.clone(),
        );
    }

    fn process_component_update(
        &mut self,
        packet_index: u16,
        global_key: &ComponentKey,
        diff_mask: &Ref<DiffMask>,
    ) -> Ref<DiffMask> {
        // previously the diff mask was the CURRENT diff mask for the
        // component, we want to lock that in so we know exactly what we're
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

        // having copied the diff mask for this update, clear the component
        self.last_popped_diff_mask = Some(diff_mask.borrow().clone());
        self.mut_handler
            .borrow_mut()
            .clear_component(&self.address, global_key);

        locked_diff_mask
    }

    fn undo_component_update(
        &mut self,
        packet_index: &u16,
        global_key: &ComponentKey,
    ) -> Ref<DiffMask> {
        if let Some(sent_updates_map) = self.sent_updates.get_mut(packet_index) {
            sent_updates_map.remove(global_key);
            if sent_updates_map.len() == 0 {
                self.sent_updates.remove(&packet_index);
            }
        }

        self.last_update_packet_index = self.last_last_update_packet_index;
        if let Some(last_popped_diff_mask) = &self.last_popped_diff_mask {
            self.mut_handler.borrow_mut().set_component(
                &self.address,
                global_key,
                &last_popped_diff_mask,
            );
        }

        self.local_component_records
            .get(*global_key)
            .expect("uh oh, we don't have enough info to unpop the action")
            .get_diff_mask()
            .clone()
    }
}

impl<P: ProtocolType> PacketNotifiable for EntityManager<P> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        let mut deleted_components: Vec<ComponentKey> = Vec::new();

        if let Some(delivered_actions_list) = self.sent_actions.remove(&packet_index) {
            for delivered_action in delivered_actions_list.into_iter() {
                match delivered_action {
                    EntityAction::RemoveComponent(global_component_key, _) => {
                        deleted_components.push(global_component_key);
                    }
                    EntityAction::UpdateComponent(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                    EntityAction::SpawnEntity(global_entity_key, _, component_list_opt) => {
                        let entity_record = self.local_entity_records.get_mut(&global_entity_key)
                            .expect("created entity does not have a entity_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_entity_deletions.remove(&global_entity_key) {
                            entity_delete(
                                &mut self.queued_actions,
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
                                        .local_component_records
                                        .get_mut(global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }
                            }

                            // for any components on this entity that have not yet been created
                            // initiate that now
                            for component_key in entity_record.get_component_keys() {
                                let component_record = self
                                    .local_component_records
                                    .get(component_key)
                                    .expect("component not created correctly?");
                                // check if component has been successfully created
                                // (perhaps through the previous entity_create operation)
                                if component_record.status == LocalityStatus::Creating {
                                    let component_ref = self
                                        .local_component_store
                                        .get(component_key)
                                        .expect("component not created correctly?");
                                    self.queued_actions.push_back(EntityAction::InsertComponent(
                                        entity_record.local_key,
                                        component_key,
                                        component_record.local_key,
                                        component_ref.clone(),
                                    ));
                                }
                            }
                        }
                    }
                    EntityAction::DespawnEntity(global_key, local_key) => {
                        let entity_record = self
                            .local_entity_records
                            .remove(&global_key)
                            .expect("deletion of nonexistent entity!");

                        // actually delete the entity from local records
                        self.local_to_global_entity_key_map.remove(&local_key);
                        self.entity_key_generator.recycle_key(&local_key);

                        // delete all components associated with entity
                        for component_key in entity_record.get_component_keys() {
                            deleted_components.push(component_key);
                        }
                    }
                    EntityAction::OwnEntity(_, _) => {}
                    EntityAction::DisownEntity(_, _) => {}
                    EntityAction::InsertComponent(_, global_component_key, _, _) => {
                        let component_record = self
                            .local_component_records
                            .get_mut(global_component_key)
                            .expect(
                                "added component does not have a record .. initiation problem?",
                            );
                        // do we need to delete this now?
                        if self
                            .delayed_component_deletions
                            .remove(&global_component_key)
                        {
                            component_delete(
                                &mut self.queued_actions,
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

        for deleted_component_key in deleted_components {
            self.component_cleanup(&deleted_component_key);
        }
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_actions_list) = self.sent_actions.get(&dropped_packet_index) {
            for dropped_action in dropped_actions_list.into_iter() {
                match dropped_action {
                    // guaranteed delivery actions
                    EntityAction::RemoveComponent(_, _)
                    | EntityAction::SpawnEntity(_, _, _)
                    | EntityAction::DespawnEntity(_, _)
                    | EntityAction::OwnEntity(_, _)
                    | EntityAction::DisownEntity(_, _)
                    | EntityAction::InsertComponent(_, _, _, _) => {
                        self.queued_actions.push_back(dropped_action.clone());
                    }
                    // non-guaranteed delivery actions
                    EntityAction::UpdateComponent(global_key, _, _, _) => {
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

                                if let Some(record) =
                                    self.local_component_records.get_mut(*global_key)
                                {
                                    let mut current_diff_mask = record.get_diff_mask().borrow_mut();
                                    current_diff_mask.or(new_diff_mask.borrow());
                                }
                            }
                        }
                    }
                }
            }

            self.sent_updates.remove(&dropped_packet_index);
            self.sent_actions.remove(&dropped_packet_index);
        }
    }
}

fn component_delete<P: ProtocolType>(
    queued_actions: &mut VecDeque<EntityAction<P>>,
    record: &mut LocalComponentRecord,
    component_key: &ComponentKey,
) {
    record.status = LocalityStatus::Deleting;

    queued_actions.push_back(EntityAction::RemoveComponent(
        *component_key,
        record.local_key,
    ));
}

fn entity_delete<P: ProtocolType>(
    queued_actions: &mut VecDeque<EntityAction<P>>,
    entity_record: &mut LocalEntityRecord,
    entity_key: &EntityKey,
) {
    entity_record.status = LocalityStatus::Deleting;

    queued_actions.push_back(EntityAction::DespawnEntity(
        *entity_key,
        entity_record.local_key,
    ));
}
