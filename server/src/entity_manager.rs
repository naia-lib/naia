use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    DiffMask, KeyGenerator, LocalComponentKey, NetEntity, PacketNotifiable, PacketWriteState,
    Protocolize, ReplicateSafe, WorldRefType,
};

use super::{
    entity_action::EntityAction, entity_action_packet_writer::EntityActionPacketWriter,
    global_diff_handler::GlobalDiffHandler, keys::ComponentKey,
    local_component_record::LocalComponentRecord, local_entity_record::LocalEntityRecord,
    locality_status::LocalityStatus, user_diff_handler::UserDiffHandler, world_record::WorldRecord,
};

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash> {
    address: SocketAddr,
    // Entities
    entity_generator: KeyGenerator<NetEntity>,
    entity_records: HashMap<E, LocalEntityRecord>,
    local_to_global_entity_map: HashMap<NetEntity, E>,
    delayed_entity_deletions: HashSet<E>,
    delayed_entity_messages: HashMap<E, VecDeque<P>>,
    // Components
    diff_handler: UserDiffHandler,
    component_key_generator: KeyGenerator<LocalComponentKey>,
    local_to_global_component_key_map: HashMap<LocalComponentKey, ComponentKey>,
    component_records: HashMap<ComponentKey, LocalComponentRecord>,
    delayed_component_deletions: HashSet<ComponentKey>,
    // Actions / updates / ect
    queued_actions: VecDeque<EntityAction<P, E>>,
    sent_actions: HashMap<u16, Vec<EntityAction<P, E>>>,
    sent_updates: HashMap<u16, HashMap<ComponentKey, DiffMask>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    last_popped_diff_mask: Option<DiffMask>,
    last_popped_diff_mask_list: Option<Vec<(ComponentKey, DiffMask)>>,
    delivered_packets: VecDeque<u16>,
    // Packet writer
    action_writer: EntityActionPacketWriter,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityManager<P, E> {
    /// Create a new EntityManager, given the client's address
    pub fn new(address: SocketAddr, diff_handler: &Arc<RwLock<GlobalDiffHandler>>) -> Self {
        EntityManager {
            address,
            // Entities
            entity_generator: KeyGenerator::new(),
            entity_records: HashMap::new(),
            local_to_global_entity_map: HashMap::new(),
            delayed_entity_deletions: HashSet::new(),
            delayed_entity_messages: HashMap::new(),
            // Components
            diff_handler: UserDiffHandler::new(diff_handler),
            component_key_generator: KeyGenerator::new(),
            local_to_global_component_key_map: HashMap::new(),
            component_records: HashMap::new(),
            delayed_component_deletions: HashSet::new(),
            // Actions / updates / ect
            queued_actions: VecDeque::new(),
            sent_actions: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ComponentKey, DiffMask>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            last_popped_diff_mask: None,
            last_popped_diff_mask_list: None,
            delivered_packets: VecDeque::new(),
            // Packet Writer
            action_writer: EntityActionPacketWriter::new(),
        }
    }

    // Entities

    pub fn spawn_entity(&mut self, world_record: &WorldRecord<E, P::Kind>, global_entity: &E) {
        if !self.entity_records.contains_key(global_entity) {
            // first, get a list of components
            // then, add components
            if !world_record.has_entity(global_entity) {
                panic!("entity nonexistant!");
            }
            for global_component_key in world_record.component_keys(global_entity) {
                self.component_init(&global_component_key, LocalityStatus::Creating);
            }

            // then, add entity
            let local_id: NetEntity = self.entity_generator.generate();
            self.local_to_global_entity_map
                .insert(local_id, *global_entity);
            let local_entity_record = LocalEntityRecord::new(local_id);
            self.entity_records
                .insert(*global_entity, local_entity_record);
            self.queued_actions
                .push_back(EntityAction::SpawnEntity(*global_entity, Vec::new()));
        } else {
            panic!("added entity twice");
        }
    }

    pub fn despawn_entity(&mut self, world_record: &WorldRecord<E, P::Kind>, global_entity: &E) {
        if let Some(entity_status) = self
            .entity_records
            .get(global_entity)
            .map(|entity_record| entity_record.status.clone())
        {
            match entity_status {
                LocalityStatus::Creating => {
                    // queue deletion action to be sent after creation
                    self.delayed_entity_deletions.insert(*global_entity);
                }
                LocalityStatus::Created => {
                    // send deletion action
                    self.entity_delete(world_record, global_entity);
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        }
    }

    pub fn has_entity(&self, entity: &E) -> bool {
        return self.entity_records.contains_key(entity);
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(&mut self, entity: &E, message: &R) {
        if let Some(entity_record) = self.entity_records.get(&entity) {
            match entity_record.status {
                LocalityStatus::Created => {
                    // send MessageEntity action
                    self.queued_actions.push_back(EntityAction::MessageEntity(
                        *entity,
                        message.protocol_copy(),
                    ));
                    return;
                }
                LocalityStatus::Deleting => {
                    return;
                }
                _ => {}
            }
        }

        // Entity hasn't been added to the User Scope yet, or replicated to Client yet
        if !self.delayed_entity_messages.contains_key(entity) {
            self.delayed_entity_messages
                .insert(*entity, VecDeque::new());
        }
        let message_queue = self.delayed_entity_messages.get_mut(entity).unwrap();
        message_queue.push_back(message.protocol_copy());
    }

    // Components

    pub fn insert_component(
        &mut self,
        world_record: &WorldRecord<E, P::Kind>,
        component_key: &ComponentKey,
    ) {
        let (entity, component_kind) = world_record
            .component_record(component_key)
            .expect("component does not exist!");

        if !self.entity_records.contains_key(&entity) {
            panic!(
                "attempting to add Component to Entity that does not yet exist for this connection"
            );
        }

        self.component_init(component_key, LocalityStatus::Creating);

        let entity_record = self.entity_records.get(&entity).unwrap(); // checked this above

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated Components will be created after Entity is
                // created
            }
            LocalityStatus::Created => {
                // send InsertComponent action
                self.queued_actions.push_back(EntityAction::InsertComponent(
                    entity,
                    *component_key,
                    component_kind,
                ));
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    pub fn remove_component(&mut self, component_key: &ComponentKey) {
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
                component_delete::<P, E>(&mut self.queued_actions, component_record, component_key);
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    // Action Writer
    pub fn peek_action_fits<W: WorldRefType<P, E>>(
        &self,
        write_state: &mut PacketWriteState,
        world_record: &WorldRecord<E, P::Kind>,
    ) -> bool {
        let queued_action_opt = self.queued_actions.front();

        if let Some(&EntityAction::SpawnEntity(global_entity, _)) = queued_action_opt {
            // get the most recent list of components in here ...
            if !world_record.has_entity(&global_entity) {
                panic!("entity does not exist!")
            }

            let mut component_list = Vec::new();

            let global_component_keys = world_record.component_keys(&global_entity);

            for global_component_key in global_component_keys {
                let (_, component_kind) = world_record
                    .component_record(&global_component_key)
                    .expect("component not tracked by server?");

                component_list.push((global_component_key, component_kind));
            }

            self.action_writer.action_fits::<P, E>(
                write_state,
                &EntityAction::SpawnEntity(global_entity, component_list),
            )
        } else {
            return if let Some(entity_action) = queued_action_opt {
                self.action_writer
                    .action_fits::<P, E>(write_state, entity_action)
            } else {
                false
            };
        }
    }

    pub fn queue_writes<W: WorldRefType<P, E>>(
        &mut self,
        write_state: &mut PacketWriteState,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        loop {
            if !self.peek_action_fits::<W>(write_state, world_record) {
                break;
            }

            let popped_entity_action = self
                .pop_outgoing_action::<W>(world_record, write_state.packet_index)
                .unwrap();
            self.action_writer.queue_write(
                write_state,
                world,
                &self.entity_records,
                &self.component_records,
                &popped_entity_action,
            );
        }
    }

    pub fn flush_writes(&mut self, out_bytes: &mut Vec<u8>) {
        self.action_writer.flush_writes(out_bytes);
    }

    // Ect..

    pub fn global_entity_from_local(&self, local_entity: NetEntity) -> Option<&E> {
        return self.local_to_global_entity_map.get(&local_entity);
    }

    pub fn collect_component_updates(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        for (component_key, record) in self.component_records.iter() {
            if record.status == LocalityStatus::Created
                && !self.diff_handler.diff_mask_is_clear(component_key)
            {
                let (entity, component_kind) = world_record
                    .component_record(component_key)
                    .expect("component does not exist!");

                let new_diff_mask = self
                    .diff_handler
                    .diff_mask(component_key)
                    .expect("DiffHandler does not have registered Component!")
                    .clone();
                self.queued_actions.push_back(EntityAction::UpdateComponent(
                    entity,
                    *component_key,
                    new_diff_mask,
                    component_kind,
                ));
            }
        }
    }

    pub fn has_outgoing_actions(&self) -> bool {
        return self.queued_actions.len() != 0;
    }

    pub fn pop_outgoing_action<W: WorldRefType<P, E>>(
        &mut self,
        world_record: &WorldRecord<E, P::Kind>,
        packet_index: u16,
    ) -> Option<EntityAction<P, E>> {
        let queued_action_opt = self.queued_actions.pop_front();
        if queued_action_opt.is_none() {
            return None;
        }
        let action = {
            let queued_action = queued_action_opt.unwrap();
            if let EntityAction::SpawnEntity(global_entity, _) = queued_action {
                // get the most recent list of components in here ...
                if !world_record.has_entity(&global_entity) {
                    panic!("entity does not exist!")
                }

                let mut component_list = Vec::new();
                let mut diff_mask_list: Vec<(ComponentKey, DiffMask)> = Vec::new();

                let global_component_keys = world_record.component_keys(&global_entity);

                for global_component_key in global_component_keys {
                    let (_, component_kind) = world_record
                        .component_record(&global_component_key)
                        .expect("component not tracked by server?");

                    component_list.push((global_component_key, component_kind));

                    let diff_mask = self
                        .diff_handler
                        .diff_mask(&global_component_key)
                        .expect("DiffHandler does not have registered Component..")
                        .clone();

                    diff_mask_list.push((global_component_key, diff_mask));

                    self.diff_handler.clear_diff_mask(&global_component_key);
                }

                self.last_popped_diff_mask_list = Some(diff_mask_list);

                EntityAction::SpawnEntity(global_entity, component_list)
            } else {
                queued_action
            }
        };

        if !self.sent_actions.contains_key(&packet_index) {
            self.sent_actions.insert(packet_index, Vec::new());
        }

        if let Some(sent_actions_list) = self.sent_actions.get_mut(&packet_index) {
            sent_actions_list.push(action.clone());
        }

        //clear diff mask of component if need be
        match action {
            EntityAction::InsertComponent(_, global_component_key, _) => {
                self.pop_insert_component_diff_mask(&global_component_key);
            }
            EntityAction::UpdateComponent(
                global_entity,
                global_component_key,
                diff_mask,
                component_kind,
            ) => {
                return Some(self.pop_update_component_diff_mask(
                    packet_index,
                    global_entity,
                    &global_component_key,
                    &diff_mask,
                    component_kind,
                ));
            }
            _ => {}
        }

        return Some(action);
    }

    // Private methods

    fn component_init(
        &mut self,
        component_key: &ComponentKey,
        status: LocalityStatus,
    ) -> LocalComponentKey {
        if self.component_records.contains_key(component_key) {
            // Should panic, as this is not dependent on any unreliable transport factor
            panic!("attempted to add component twice..");
        }

        // create DiffMask
        self.diff_handler
            .register_component(&self.address, &component_key);

        // register Component with various indexes
        let local_component_key: LocalComponentKey = self.component_key_generator.generate();
        self.local_to_global_component_key_map
            .insert(local_component_key, *component_key);
        let component_record = LocalComponentRecord::new(local_component_key, status);
        self.component_records
            .insert(*component_key, component_record);
        return local_component_key;
    }

    fn component_cleanup(&mut self, global_component_key: &ComponentKey) {
        if let Some(component_record) = self.component_records.remove(global_component_key) {
            // actually delete the component from local records
            self.diff_handler.deregister_component(global_component_key);

            let local_component_key = component_record.local_key;
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

    fn pop_insert_component_diff_mask(&mut self, global_component_key: &ComponentKey) {
        let new_diff_mask = self
            .diff_handler
            .diff_mask(global_component_key)
            .expect("DiffHandler doesn't have Component registered!")
            .clone();
        self.last_popped_diff_mask = Some(new_diff_mask);
        self.diff_handler.clear_diff_mask(global_component_key);
    }

    fn pop_update_component_diff_mask(
        &mut self,
        packet_index: u16,
        global_entity: E,
        global_component_key: &ComponentKey,
        diff_mask: &DiffMask,
        component_kind: P::Kind,
    ) -> EntityAction<P, E> {
        let locked_diff_mask =
            self.process_component_update(packet_index, global_component_key, diff_mask);
        // return new Update action to be written
        return EntityAction::UpdateComponent(
            global_entity,
            *global_component_key,
            locked_diff_mask,
            component_kind,
        );
    }

    fn process_component_update(
        &mut self,
        packet_index: u16,
        global_component_key: &ComponentKey,
        diff_mask: &DiffMask,
    ) -> DiffMask {
        // previously the diff mask was the CURRENT diff mask for the
        // component, we want to lock that in so we know exactly what we're
        // writing
        let locked_diff_mask = diff_mask.clone();

        // place diff mask in a special transmission record - like map
        if !self.sent_updates.contains_key(&packet_index) {
            let sent_updates_map: HashMap<ComponentKey, DiffMask> = HashMap::new();
            self.sent_updates.insert(packet_index, sent_updates_map);
            self.last_last_update_packet_index = self.last_update_packet_index;
            self.last_update_packet_index = packet_index;
        }

        if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
            sent_updates_map.insert(*global_component_key, locked_diff_mask.clone());
        }

        // having copied the diff mask for this update, clear the component
        self.last_popped_diff_mask = Some(diff_mask.borrow().clone());
        self.diff_handler.clear_diff_mask(global_component_key);

        locked_diff_mask
    }

    pub fn process_delivered_packets(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        while let Some(packet_index) = self.delivered_packets.pop_front() {
            let mut deleted_components: Vec<ComponentKey> = Vec::new();

            if let Some(delivered_actions_list) = self.sent_actions.remove(&packet_index) {
                for delivered_action in delivered_actions_list.into_iter() {
                    match delivered_action {
                        EntityAction::RemoveComponent(global_component_key) => {
                            deleted_components.push(global_component_key);
                        }
                        EntityAction::UpdateComponent(_, _, _, _) => {
                            self.sent_updates.remove(&packet_index);
                        }
                        EntityAction::SpawnEntity(global_entity, mut component_list) => {
                            let entity_record = self.entity_records.get_mut(&global_entity)
                                .expect("created entity does not have a entity_record ... initialization error?");

                            // do we need to delete this now?
                            if self.delayed_entity_deletions.remove(&global_entity) {
                                self.entity_delete(world_record, &global_entity);
                            } else {
                                // set to status of Entity to Created
                                entity_record.status = LocalityStatus::Created;

                                // set status of Components to Created
                                while let Some((global_component_key, _)) = component_list.pop() {
                                    let component_record = self
                                        .component_records
                                        .get_mut(&global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }

                                // for any components on this entity that have not yet been created
                                // initiate that now
                                for global_component_key in
                                    world_record.component_keys(&global_entity)
                                {
                                    let component_record = self
                                        .component_records
                                        .get(&global_component_key)
                                        .expect("component not created correctly?");
                                    // check if component has been successfully created
                                    // (perhaps through the previous entity_create operation)
                                    if component_record.status == LocalityStatus::Creating {
                                        let (_, component_kind) = world_record
                                            .component_record(&global_component_key)
                                            .expect("component does not exist!");

                                        self.queued_actions.push_back(
                                            EntityAction::InsertComponent(
                                                global_entity,
                                                global_component_key,
                                                component_kind,
                                            ),
                                        );
                                    }
                                }

                                // send any Entity messages that have been waiting
                                if let Some(message_queue) =
                                    self.delayed_entity_messages.get_mut(&global_entity)
                                {
                                    while let Some(message) = message_queue.pop_front() {
                                        self.queued_actions.push_back(EntityAction::MessageEntity(
                                            global_entity,
                                            message,
                                        ));
                                    }
                                }
                            }
                        }
                        EntityAction::DespawnEntity(global_entity) => {
                            let local_id = self
                                .entity_records
                                .get(&global_entity)
                                .unwrap()
                                .entity_net_id;

                            // actually delete the entity from local records
                            self.entity_records.remove(&global_entity);
                            self.delayed_entity_messages.remove(&global_entity);
                            self.local_to_global_entity_map.remove(&local_id);
                            self.entity_generator.recycle_key(&local_id);
                        }
                        EntityAction::MessageEntity(_, _) => {
                            // Don't do anything, mission accomplished
                        }
                        EntityAction::InsertComponent(_, global_component_key, _) => {
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
                                component_delete::<P, E>(
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

    fn entity_delete(&mut self, world_record: &WorldRecord<E, P::Kind>, entity: &E) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            entity_record.status = LocalityStatus::Deleting;

            // Entity deletion IS Component deletion, so update those component records
            // accordingly
            for global_component_key in world_record.component_keys(entity) {
                self.component_cleanup(&global_component_key);
            }

            self.queued_actions
                .push_back(EntityAction::DespawnEntity(*entity));
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> PacketNotifiable for EntityManager<P, E> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.delivered_packets.push_back(packet_index);
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_actions_list) = self.sent_actions.get_mut(&dropped_packet_index) {
            for dropped_action in dropped_actions_list.drain(..) {
                match dropped_action {
                    // guaranteed delivery actions
                    EntityAction::SpawnEntity(_, _)
                    | EntityAction::DespawnEntity(_)
                    | EntityAction::MessageEntity(_, _)
                    | EntityAction::InsertComponent(_, _, _)
                    | EntityAction::RemoveComponent(_) => {
                        self.queued_actions.push_back(dropped_action);
                    }
                    // non-guaranteed delivery actions
                    EntityAction::UpdateComponent(_, global_component_key, _, _) => {
                        if let Some(diff_mask_map) = self.sent_updates.get(&dropped_packet_index) {
                            if let Some(diff_mask) = diff_mask_map.get(&global_component_key) {
                                let mut new_diff_mask = diff_mask.borrow().clone();

                                // walk from dropped packet up to most recently sent packet
                                if dropped_packet_index != self.last_update_packet_index {
                                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                                    while packet_index != self.last_update_packet_index {
                                        if let Some(diff_mask_map) =
                                            self.sent_updates.get(&packet_index)
                                        {
                                            if let Some(diff_mask) =
                                                diff_mask_map.get(&global_component_key)
                                            {
                                                new_diff_mask.nand(diff_mask.borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                self.diff_handler
                                    .or_diff_mask(&global_component_key, &new_diff_mask);
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

fn component_delete<P: Protocolize, E: Copy>(
    queued_actions: &mut VecDeque<EntityAction<P, E>>,
    record: &mut LocalComponentRecord,
    component_key: &ComponentKey,
) {
    record.status = LocalityStatus::Deleting;

    queued_actions.push_back(EntityAction::RemoveComponent(*component_key));
}
