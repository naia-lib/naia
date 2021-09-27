use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{
    DiffMask, KeyGenerator, LocalComponentKey, LocalEntityKey, Manifest, NaiaKey, PacketNotifiable,
    ProtocolType, Ref, Replicate, MTU_SIZE,
};

use crate::packet_writer::PacketWriter;

use super::{
    entity_action::EntityAction, keys::ComponentKey, local_component_record::LocalComponentRecord,
    local_entity_record::LocalEntityRecord, locality_status::LocalityStatus,
    mut_handler::MutHandler, world_type::WorldType,
};

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
#[derive(Debug)]
pub struct EntityManager<P: ProtocolType, W: WorldType<P>> {
    address: SocketAddr,
    // Entities
    entity_key_generator: KeyGenerator<LocalEntityKey>,
    entity_records: HashMap<W::EntityKey, LocalEntityRecord>,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, W::EntityKey>,
    delayed_entity_deletions: HashSet<W::EntityKey>,
    // Components
    component_key_generator: KeyGenerator<LocalComponentKey>,
    local_to_global_component_key_map: HashMap<LocalComponentKey, ComponentKey<W::EntityKey>>,
    component_records: HashMap<ComponentKey<W::EntityKey>, LocalComponentRecord>,
    delayed_component_deletions: HashSet<ComponentKey<W::EntityKey>>,
    // Actions / updates / ect
    queued_actions: VecDeque<EntityAction<P, W::EntityKey>>,
    sent_actions: HashMap<u16, Vec<EntityAction<P, W::EntityKey>>>,
    sent_updates: HashMap<u16, HashMap<ComponentKey<W::EntityKey>, Ref<DiffMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler<W::EntityKey>>,
    last_popped_diff_mask: Option<DiffMask>,
    last_popped_diff_mask_list: Option<Vec<(ComponentKey<W::EntityKey>, DiffMask)>>,
    delivered_packets: VecDeque<u16>,
}

impl<P: ProtocolType, W: WorldType<P>> EntityManager<P, W> {
    /// Create a new EntityManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler<W::EntityKey>>) -> Self {
        EntityManager {
            address,
            // Entities
            entity_key_generator: KeyGenerator::new(),
            entity_records: HashMap::new(),
            local_to_global_entity_key_map: HashMap::new(),
            delayed_entity_deletions: HashSet::new(),
            // Components
            component_key_generator: KeyGenerator::new(),
            local_to_global_component_key_map: HashMap::new(),
            component_records: HashMap::new(),
            delayed_component_deletions: HashSet::new(),
            // Actions / updates / ect
            queued_actions: VecDeque::new(),
            sent_actions: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ComponentKey<W::EntityKey>, Ref<DiffMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_diff_mask: None,
            last_popped_diff_mask_list: None,
            delivered_packets: VecDeque::new(),
        }
    }

    pub fn has_outgoing_actions(&self) -> bool {
        return self.queued_actions.len() != 0;
    }

    pub fn pop_outgoing_action(
        &mut self,
        world: &W,
        packet_index: u16,
    ) -> Option<EntityAction<P, W::EntityKey>> {
        let queued_action_opt = self.queued_actions.pop_front();
        if queued_action_opt.is_none() {
            return None;
        }
        let mut action = queued_action_opt.unwrap();

        let replacement_action: Option<EntityAction<P, W::EntityKey>> = {
            match &action {
                EntityAction::SpawnEntity(global_entity_key, local_entity_key, _) => {
                    let mut component_list = Vec::new();

                    for component_protocol in world.get_components(global_entity_key) {
                        let component_ref = component_protocol.inner_ref();
                        let component_key = ComponentKey::new(
                            global_entity_key,
                            &component_ref.borrow().get_type_id(),
                        );
                        let component_record = self.component_records.get(&component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        component_list.push((
                            *component_key.component_type(),
                            component_record.local_key,
                            component_ref.clone(),
                        ));
                    }

                    Some(EntityAction::SpawnEntity(
                        *global_entity_key,
                        *local_entity_key,
                        component_list,
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
            EntityAction::SpawnEntity(entity_key, _, components_list) => {
                let mut diff_mask_list: Vec<(ComponentKey<W::EntityKey>, DiffMask)> = Vec::new();
                for (type_id, _, _) in components_list {
                    let global_component_key = ComponentKey::new(entity_key, type_id);
                    if let Some(record) = self.component_records.get(&global_component_key) {
                        diff_mask_list.push((
                            global_component_key,
                            record.get_diff_mask().borrow().clone(),
                        ));
                    }
                    self.mut_handler
                        .borrow_mut()
                        .clear_component(&self.address, &global_component_key);
                }
                self.last_popped_diff_mask_list = Some(diff_mask_list);
            }
            EntityAction::InsertComponent(_, global_key, _, _) => {
                self.pop_insert_component_diff_mask(global_key);
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

    pub fn unpop_outgoing_action(
        &mut self,
        packet_index: u16,
        action: &EntityAction<P, W::EntityKey>,
    ) {
        info!("unpopping");
        if let Some(sent_actions_list) = self.sent_actions.get_mut(&packet_index) {
            sent_actions_list.pop();
            if sent_actions_list.len() == 0 {
                self.sent_actions.remove(&packet_index);
            }
        }

        match &action {
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
            EntityAction::InsertComponent(_, global_key, _, _) => {
                self.unpop_insert_component_diff_mask(global_key);
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

    pub fn add_entity(&mut self, world: &W, global_entity_key: &W::EntityKey) {
        if !self.entity_records.contains_key(global_entity_key) {
            // first, get a list of components
            // then, add components
            for component_protocol in world.get_components(global_entity_key) {
                let inner_ref = component_protocol.inner_ref();
                let diff_mask_size = inner_ref.borrow().get_diff_mask_size();
                let type_id = inner_ref.borrow().get_type_id();
                let component_key = ComponentKey::new(global_entity_key, &type_id);
                self.component_init(&component_key, diff_mask_size, LocalityStatus::Creating);
            }

            // then, add entity
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map
                .insert(local_key, *global_entity_key);
            let local_entity_record = LocalEntityRecord::new(local_key);
            self.entity_records
                .insert(*global_entity_key, local_entity_record);
            self.queued_actions.push_back(EntityAction::SpawnEntity(
                *global_entity_key,
                local_key,
                Vec::new(),
            ));
        } else {
            panic!("added entity twice");
        }
    }

    pub fn remove_entity(&mut self, world: &W, key: &W::EntityKey) {
        if self.has_entity_prediction(key) {
            self.remove_prediction_entity(key);
        }

        if let Some(entity_record) = self.entity_records.get_mut(key) {
            match entity_record.status {
                LocalityStatus::Creating => {
                    // queue deletion action to be sent after creation
                    self.delayed_entity_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion action
                    entity_delete::<P, W>(&mut self.queued_actions, entity_record, key);

                    // Entity deletion IS Component deletion, so update those component records
                    // accordingly
                    for component_protocol in world.get_components(key) {
                        let component_key =
                            ComponentKey::new(key, &component_protocol.get_type_id());
                        if let Some(component_record) =
                            self.component_records.get_mut(&component_key)
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

    pub fn has_entity(&self, key: &W::EntityKey) -> bool {
        return self.entity_records.contains_key(key);
    }

    // Prediction Entities

    pub fn add_prediction_entity(&mut self, key: &W::EntityKey) {
        let entity_record = self
            .entity_records
            .get_mut(key)
            .expect("attempting to assign a nonexistent Entity");
        if entity_record.is_prediction {
            panic!("attempting to assign an Entity twice!");
        }

        // success
        entity_record.is_prediction = true;
        self.queued_actions
            .push_back(EntityAction::OwnEntity(*key, entity_record.local_key));
    }

    pub fn remove_prediction_entity(&mut self, key: &W::EntityKey) {
        let entity_record = self
            .entity_records
            .get_mut(key)
            .expect("attempting to disown on Entity which is not in-scope");
        if !entity_record.is_prediction {
            panic!("attempting to disown an Entity which is not currently assigned");
        }

        // success
        entity_record.is_prediction = false;
        self.queued_actions
            .push_back(EntityAction::DisownEntity(*key, entity_record.local_key));
    }

    pub fn has_entity_prediction(&self, key: &W::EntityKey) -> bool {
        if let Some(entity_record) = self.entity_records.get(key) {
            return entity_record.is_prediction;
        }
        return false;
    }

    // Components

    pub fn insert_component(&mut self, world: &W, component_key: &ComponentKey<W::EntityKey>) {
        let entity_key = component_key.entity_key();
        if let Some(component_protocol) = world.get_component_from_key(&component_key) {
            let component_ref = component_protocol.inner_ref();

            if !self.entity_records.contains_key(entity_key) {
                panic!(
                    "attempting to add Component to Entity that does not yet exist for this connection"
                );
            }

            let local_component_key = self.component_init(
                component_key,
                component_ref.borrow().get_diff_mask_size(),
                LocalityStatus::Creating,
            );

            let entity_record = self.entity_records.get(entity_key).unwrap(); // checked this above

            match entity_record.status {
                LocalityStatus::Creating => {
                    // uncreated Components will be created after Entity is
                    // created
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
    }

    pub fn remove_component(&mut self, component_key: &ComponentKey<W::EntityKey>) {
        let component_record = self.component_records.get_mut(component_key).expect(
            "attempting to remove a component from a connection within which it does not exist",
        );

        match component_record.status {
            LocalityStatus::Creating => {
                // queue deletion action to be sent after creation
                self.delayed_component_deletions.insert(*component_key);
            }
            LocalityStatus::Created => {
                // send deletion action
                component_delete::<P, W>(&mut self.queued_actions, component_record, component_key);
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
    ) -> Option<&W::EntityKey> {
        return self.local_to_global_entity_key_map.get(&local_key);
    }

    pub fn collect_component_updates(&mut self, world: &W) {
        for (key, record) in self.component_records.iter() {
            if record.status == LocalityStatus::Created
                && !record.get_diff_mask().borrow().is_clear()
            {
                if let Some(component_protocol) = world.get_component_from_key(key) {
                    self.queued_actions.push_back(EntityAction::UpdateComponent(
                        *key,
                        record.local_key,
                        record.get_diff_mask().clone(),
                        component_protocol.inner_ref(),
                    ));
                }
            }
        }
    }

    pub fn write_entity_action(
        &self,
        packet_writer: &mut PacketWriter,
        manifest: &Manifest<P>,
        action: &EntityAction<P, W::EntityKey>,
    ) -> bool {
        let mut action_total_bytes = Vec::<u8>::new();

        //Write EntityAction type
        action_total_bytes
            .write_u8(action.as_type().to_u8())
            .unwrap();

        match action {
            EntityAction::SpawnEntity(_, local_entity_key, component_list) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key

                // get list of components
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
            EntityAction::RemoveComponent(_, local_key) => {
                action_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
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
        component_key: &ComponentKey<W::EntityKey>,
        diff_mask_size: u8,
        status: LocalityStatus,
    ) -> LocalComponentKey {
        if self.component_records.contains_key(component_key) {
            // Should panic, as this is not dependent on any unreliable transport factor
            panic!("attempted to add component twice..");
        }

        let local_key: LocalComponentKey = self.component_key_generator.generate();
        self.local_to_global_component_key_map
            .insert(local_key, *component_key);
        let component_record = LocalComponentRecord::new(local_key, diff_mask_size, status);
        self.mut_handler.borrow_mut().register_mask(
            &self.address,
            &component_key,
            component_record.get_diff_mask(),
        );
        self.component_records
            .insert(*component_key, component_record);
        return local_key;
    }

    fn component_cleanup(&mut self, global_component_key: &ComponentKey<W::EntityKey>) {
        if let Some(component_record) = self.component_records.remove(global_component_key) {
            // actually delete the component from local records
            let local_component_key = component_record.local_key;
            self.mut_handler
                .borrow_mut()
                .deregister_mask(&self.address, global_component_key);
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

    fn pop_insert_component_diff_mask(&mut self, global_key: &ComponentKey<W::EntityKey>) {
        if let Some(record) = self.component_records.get(global_key) {
            self.last_popped_diff_mask = Some(record.get_diff_mask().borrow().clone());
        }
        self.mut_handler
            .borrow_mut()
            .clear_component(&self.address, global_key);
    }

    fn unpop_insert_component_diff_mask(&mut self, global_key: &ComponentKey<W::EntityKey>) {
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
        global_key: &ComponentKey<W::EntityKey>,
        local_key: &LocalComponentKey,
        diff_mask: &Ref<DiffMask>,
        component: &Ref<dyn Replicate<P>>,
    ) -> EntityAction<P, W::EntityKey> {
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
        global_key: &ComponentKey<W::EntityKey>,
        local_key: &LocalComponentKey,
        component: &Ref<dyn Replicate<P>>,
    ) -> EntityAction<P, W::EntityKey> {
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
        global_key: &ComponentKey<W::EntityKey>,
        diff_mask: &Ref<DiffMask>,
    ) -> Ref<DiffMask> {
        // previously the diff mask was the CURRENT diff mask for the
        // component, we want to lock that in so we know exactly what we're
        // writing
        let locked_diff_mask = Ref::new(diff_mask.borrow().clone());

        // place diff mask in a special transmission record - like map
        if !self.sent_updates.contains_key(&packet_index) {
            let sent_updates_map: HashMap<ComponentKey<W::EntityKey>, Ref<DiffMask>> =
                HashMap::new();
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
        global_key: &ComponentKey<W::EntityKey>,
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

        self.component_records
            .get(global_key)
            .expect("uh oh, we don't have enough info to unpop the action")
            .get_diff_mask()
            .clone()
    }

    pub fn process_delivered_packets(&mut self, world: &W) {
        while let Some(packet_index) = self.delivered_packets.pop_front() {
            let mut deleted_components: Vec<ComponentKey<W::EntityKey>> = Vec::new();

            if let Some(delivered_actions_list) = self.sent_actions.remove(&packet_index) {
                for delivered_action in delivered_actions_list.into_iter() {
                    match delivered_action {
                        EntityAction::RemoveComponent(global_component_key, _) => {
                            deleted_components.push(global_component_key);
                        }
                        EntityAction::UpdateComponent(_, _, _, _) => {
                            self.sent_updates.remove(&packet_index);
                        }
                        EntityAction::SpawnEntity(global_entity_key, _, mut component_list) => {
                            let entity_record = self.entity_records.get_mut(&global_entity_key)
                                .expect("created entity does not have a entity_record ... initialization error?");

                            // do we need to delete this now?
                            if self.delayed_entity_deletions.remove(&global_entity_key) {
                                entity_delete::<P, W>(
                                    &mut self.queued_actions,
                                    entity_record,
                                    &global_entity_key,
                                );
                            } else {
                                // set to status of created
                                entity_record.status = LocalityStatus::Created;

                                // set status of components to created
                                while let Some((component_type, _, _)) = component_list.pop() {
                                    let global_component_key =
                                        ComponentKey::new(&global_entity_key, &component_type);
                                    let component_record = self
                                        .component_records
                                        .get_mut(&global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }

                                // for any components on this entity that have not yet been created
                                // initiate that now
                                for component_protocol in world.get_components(&global_entity_key) {
                                    let component_key = ComponentKey::new(
                                        &global_entity_key,
                                        &component_protocol.get_type_id(),
                                    );
                                    let component_record = self
                                        .component_records
                                        .get(&component_key)
                                        .expect("component not created correctly?");
                                    // check if component has been successfully created
                                    // (perhaps through the previous entity_create operation)
                                    if component_record.status == LocalityStatus::Creating {
                                        self.queued_actions.push_back(
                                            EntityAction::InsertComponent(
                                                entity_record.local_key,
                                                component_key,
                                                component_record.local_key,
                                                component_protocol.inner_ref(),
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        EntityAction::DespawnEntity(global_key, local_key) => {
                            // actually delete the entity from local records
                            self.local_to_global_entity_key_map.remove(&local_key);
                            self.entity_key_generator.recycle_key(&local_key);

                            // delete all components associated with entity
                            for component_protocol in world.get_components(&global_key) {
                                let component_key = ComponentKey::new(
                                    &global_key,
                                    &component_protocol.get_type_id(),
                                );
                                deleted_components.push(component_key);
                            }
                        }
                        EntityAction::OwnEntity(_, _) => {}
                        EntityAction::DisownEntity(_, _) => {}
                        EntityAction::InsertComponent(_, global_component_key, _, _) => {
                            let component_record = self
                                .component_records
                                .get_mut(&global_component_key)
                                .expect(
                                    "added component does not have a record .. initiation problem?",
                                );
                            // do we need to delete this now?
                            if self
                                .delayed_component_deletions
                                .remove(&global_component_key)
                            {
                                component_delete::<P, W>(
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
    }
}

impl<P: ProtocolType, W: WorldType<P>> PacketNotifiable for EntityManager<P, W> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.delivered_packets.push_back(packet_index);
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

                                if let Some(record) = self.component_records.get_mut(global_key) {
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

fn component_delete<P: ProtocolType, W: WorldType<P>>(
    queued_actions: &mut VecDeque<EntityAction<P, W::EntityKey>>,
    record: &mut LocalComponentRecord,
    component_key: &ComponentKey<W::EntityKey>,
) {
    record.status = LocalityStatus::Deleting;

    queued_actions.push_back(EntityAction::RemoveComponent(
        *component_key,
        record.local_key,
    ));
}

fn entity_delete<P: ProtocolType, W: WorldType<P>>(
    queued_actions: &mut VecDeque<EntityAction<P, W::EntityKey>>,
    entity_record: &mut LocalEntityRecord,
    entity_key: &W::EntityKey,
) {
    entity_record.status = LocalityStatus::Deleting;

    queued_actions.push_back(EntityAction::DespawnEntity(
        *entity_key,
        entity_record.local_key,
    ));
}
