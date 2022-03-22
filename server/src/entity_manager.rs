use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use crate::entity_message_waitlist::EntityMessageWaitlist;
use naia_shared::{
    serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger},
    write_list_header, ChannelIndex, DiffMask, EntityConverter, KeyGenerator, MessageManager,
    NetEntity, NetEntityConverter, PacketIndex, PacketNotifiable, Protocolize, ReplicateSafe,
    WorldRefType, MTU_SIZE_BITS,
};

use super::{
    entity_action::EntityAction, global_diff_handler::GlobalDiffHandler,
    local_entity_record::LocalEntityRecord, locality_status::LocalityStatus,
    user_diff_handler::UserDiffHandler, world_record::WorldRecord,
};

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    address: SocketAddr,
    // Entities
    entity_generator: KeyGenerator<NetEntity>,
    entity_records: HashMap<E, LocalEntityRecord<P>>,
    local_to_global_entity_map: HashMap<NetEntity, E>,
    delayed_entity_deletions: HashSet<E>,
    delayed_entity_messages: EntityMessageWaitlist<P, E, C>,
    // Components
    diff_handler: UserDiffHandler<E, P::Kind>,
    delayed_component_deletions: HashSet<(E, P::Kind)>,
    // Actions / updates / ect
    queued_actions: VecDeque<EntityAction<P, E>>,
    sent_actions: HashMap<PacketIndex, Vec<EntityAction<P, E>>>,
    sent_updates: HashMap<PacketIndex, HashMap<(E, P::Kind), DiffMask>>,
    last_update_packet_index: PacketIndex,
    delivered_packets: VecDeque<PacketIndex>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> EntityManager<P, E, C> {
    /// Create a new EntityManager, given the client's address
    pub fn new(
        address: SocketAddr,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E, P::Kind>>>,
    ) -> Self {
        EntityManager {
            address,
            // Entities
            entity_generator: KeyGenerator::new(),
            entity_records: HashMap::new(),
            local_to_global_entity_map: HashMap::new(),
            delayed_entity_deletions: HashSet::new(),
            delayed_entity_messages: EntityMessageWaitlist::new(),
            // Components
            diff_handler: UserDiffHandler::new(diff_handler),
            delayed_component_deletions: HashSet::new(),
            // Actions / updates / ect
            queued_actions: VecDeque::new(),
            sent_actions: HashMap::new(),
            sent_updates: HashMap::new(),
            last_update_packet_index: 0,
            delivered_packets: VecDeque::new(),
        }
    }

    // Entities

    pub fn spawn_entity(&mut self, world_record: &WorldRecord<E, P::Kind>, global_entity: &E) {
        if !self.entity_records.contains_key(global_entity) {
            // initialize entity
            if !world_record.has_entity(global_entity) {
                panic!("entity nonexistant!");
            }
            let local_id: NetEntity = self.entity_generator.generate();
            self.local_to_global_entity_map
                .insert(local_id, *global_entity);
            let local_entity_record = LocalEntityRecord::new(local_id);
            self.entity_records
                .insert(*global_entity, local_entity_record);

            // now initialize components
            for component_kind in world_record.component_kinds(global_entity) {
                self.component_init(global_entity, &component_kind);
            }

            self.queued_actions
                .push_back(EntityAction::SpawnEntity(*global_entity, None));
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

    pub fn entity_in_scope(&self, entity: &E) -> bool {
        if let Some(entity_record) = self.entity_records.get(&entity) {
            if entity_record.status == LocalityStatus::Created {
                return true;
            }
        }
        return false;
    }

    pub fn queue_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entities: Vec<E>,
        channel: C,
        message: &R,
    ) {
        self.delayed_entity_messages
            .queue_message(entities, channel, message.protocol_copy());
    }

    pub fn collect_entity_messages(&mut self, message_manager: &mut MessageManager<P, C>) {
        self.delayed_entity_messages
            .collect_ready_messages(message_manager);
    }

    // Components

    pub fn insert_component(&mut self, entity: &E, component_kind: &P::Kind) {
        if !self.entity_records.contains_key(&entity) {
            panic!(
                "attempting to add Component to Entity that does not yet exist for this connection"
            );
        }

        self.component_init(entity, component_kind);

        // checked this above
        let entity_record = self.entity_records.get(&entity).unwrap();

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated Components will be created after Entity is
                // created
            }
            LocalityStatus::Created => {
                // send InsertComponent action
                self.queued_actions
                    .push_back(EntityAction::InsertComponent(*entity, *component_kind));
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    pub fn remove_component(&mut self, entity: &E, component_kind: &P::Kind) {
        let component_status: LocalityStatus = {
            if let Some(entity_record) = self.entity_records.get(&entity) {
                if let Some(status) = entity_record.components.get(component_kind) {
                    *status
                } else {
                    panic!("attempting to remove non-existent Component from Entity");
                }
            } else {
                panic!(
                    "attempting to remove Component from Entity that does not exist for this connection"
                );
            }
        };

        match component_status {
            LocalityStatus::Creating => {
                // queue deletion action to be sent after creation
                self.delayed_component_deletions
                    .insert((*entity, *component_kind));
            }
            LocalityStatus::Created => {
                // send deletion action
                self.component_delete(entity, component_kind);
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    // Action Writer

    pub fn write_actions<W: WorldRefType<P, E>>(
        &mut self,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                return;
            }

            let mut counter = BitCounter::new();
            write_list_header(&mut counter, &123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                return;
            }

            // Find how many messages will fit into the packet
            let queued_actions_len = self.queued_actions.len();
            for action_index in 0..queued_actions_len {
                self.write_action(
                    &mut counter,
                    packet_index,
                    world,
                    world_record,
                    Some(action_index),
                );
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }
            }
        }

        // Write header
        write_list_header(writer, &message_count);

        // Actions
        {
            for _ in 0..message_count {
                // Pop message

                // Write message
                self.write_action(writer, packet_index, world, world_record, None);
            }
        }
    }

    pub fn write_action<W: WorldRefType<P, E>, S: BitWrite>(
        &mut self,
        writer: &mut S,
        packet_index: PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
        action_index: Option<usize>,
    ) {
        let is_writing: bool = action_index.is_none();
        let mut action_holder: Option<EntityAction<P, E>> = None;
        if is_writing {
            action_holder = Some(
                self.queued_actions
                    .pop_front()
                    .expect("should be an action available to pop"),
            );
        }
        let action = {
            if is_writing {
                action_holder.as_ref().unwrap()
            } else {
                let open_action_index = action_index.unwrap();
                self.queued_actions.get(open_action_index).as_ref().unwrap()
            }
        };

        if !self.sent_actions.contains_key(&packet_index) {
            self.sent_actions.insert(packet_index, Vec::new());
        }

        //Write EntityAction type
        action.as_type().ser(writer);

        match action {
            EntityAction::SpawnEntity(global_entity, _) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(writer);

                // get component list
                let mut component_kinds = Vec::new();
                for component_kind in world_record.component_kinds(&global_entity) {
                    component_kinds.push(component_kind);
                }

                // write number of components
                let components_num =
                    UnsignedVariableInteger::<3>::new(component_kinds.len() as i128);
                components_num.ser(writer);

                for component_kind in &component_kinds {
                    // write kind
                    component_kind.ser(writer);

                    // write payload
                    let component = world
                        .component_of_kind(global_entity, &component_kind)
                        .expect("Component does not exist in World");

                    {
                        let converter = EntityConverter::new(world_record, self);
                        component.write(writer, &converter);
                    }

                    // only clear diff mask if we are actually writing the packet
                    if is_writing {
                        self.diff_handler
                            .clear_diff_mask(global_entity, &component_kind);
                    }
                }

                // write to record, if we are writing to this packet
                if is_writing {
                    let sent_actions_list = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push(EntityAction::SpawnEntity(
                        *global_entity,
                        Some(component_kinds),
                    ));
                }
            }
            EntityAction::DespawnEntity(global_entity) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(writer);

                // write to record, if we are writing to this packet
                if is_writing {
                    let sent_actions_list = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push(EntityAction::DespawnEntity(*global_entity));
                }
            }
            EntityAction::InsertComponent(global_entity, component_kind) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(writer);

                // write component kind
                component_kind.ser(writer);

                // write component payload
                let component = world
                    .component_of_kind(global_entity, component_kind)
                    .expect("Component does not exist in World");

                {
                    let converter = EntityConverter::new(world_record, self);
                    component.write(writer, &converter);
                }

                // if we are actually writing this packet
                if is_writing {
                    // clear the component's diff mask
                    self.diff_handler
                        .clear_diff_mask(&global_entity, component_kind);

                    // write to record
                    let sent_actions_list = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push(EntityAction::InsertComponent(
                        *global_entity,
                        *component_kind,
                    ));
                }
            }
            EntityAction::UpdateComponent(global_entity, component_kind) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(writer);

                // write component kind
                component_kind.ser(writer);

                // get diff mask
                let diff_mask = self
                    .diff_handler
                    .diff_mask(global_entity, component_kind)
                    .expect("DiffHandler does not have registered Component!")
                    .clone();

                // write payload
                {
                    let converter = EntityConverter::new(world_record, self);
                    world
                        .component_of_kind(global_entity, component_kind)
                        .expect("Component does not exist in World")
                        .write_partial(&diff_mask, writer, &converter);
                }

                ////////
                if is_writing {
                    // place diff mask in a special transmission record - like map
                    if !self.sent_updates.contains_key(&packet_index) {
                        self.sent_updates.insert(packet_index, HashMap::new());
                        self.last_update_packet_index = packet_index;
                    }

                    let sent_updates_map = self.sent_updates.get_mut(&packet_index).unwrap();
                    sent_updates_map.insert((*global_entity, *component_kind), diff_mask);

                    // having copied the diff mask for this update, clear the component
                    self.diff_handler
                        .clear_diff_mask(global_entity, component_kind);

                    let sent_actions_list = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push(EntityAction::UpdateComponent(
                        *global_entity,
                        *component_kind,
                    ));
                }
            }
            EntityAction::RemoveComponent(global_entity, component_kind) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(writer);

                // write component kind
                component_kind.ser(writer);

                // if we are writing to this packet
                if is_writing {
                    // write to record
                    let sent_actions_list = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push(EntityAction::RemoveComponent(
                        *global_entity,
                        *component_kind,
                    ));
                }
            }
        }
    }

    // Ect..

    pub fn global_entity_from_local(&self, local_entity: NetEntity) -> Option<&E> {
        return self.local_to_global_entity_map.get(&local_entity);
    }

    pub fn collect_component_updates(&mut self) {
        for (global_entity, entity_record) in self.entity_records.iter() {
            for (component_kind, locality_status) in entity_record.components.iter() {
                if *locality_status == LocalityStatus::Created
                    && !self
                        .diff_handler
                        .diff_mask_is_clear(global_entity, component_kind)
                {
                    self.queued_actions.push_back(EntityAction::UpdateComponent(
                        *global_entity,
                        *component_kind,
                    ));
                }
            }
        }
    }

    pub fn has_outgoing_actions(&self) -> bool {
        return self.queued_actions.len() != 0;
    }

    // Private methods

    fn component_init(&mut self, entity: &E, component_kind: &P::Kind) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            if entity_record.components.contains_key(component_kind) {
                panic!("entity already has a component of the given type!");
            }
            entity_record
                .components
                .insert(*component_kind, LocalityStatus::Creating);
        } else {
            panic!("entity does not exist!");
        }

        // create DiffMask
        self.diff_handler
            .register_component(&self.address, entity, component_kind);
    }

    fn component_cleanup(&mut self, entity: &E, component_kind: &P::Kind) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            // actually delete the component from local records
            entity_record.components.remove(component_kind);

            self.diff_handler
                .deregister_component(entity, component_kind);
        } else {
            panic!("attempting to clean up component from non-existent entity!")
        }
    }

    pub fn process_delivered_packets(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        while let Some(packet_index) = self.delivered_packets.pop_front() {
            let mut deleted_components: Vec<(E, P::Kind)> = Vec::new();

            if let Some(delivered_actions_list) = self.sent_actions.remove(&packet_index) {
                for delivered_action in delivered_actions_list.into_iter() {
                    match delivered_action {
                        EntityAction::RemoveComponent(global_entity, component_kind) => {
                            deleted_components.push((global_entity, component_kind));
                        }
                        EntityAction::UpdateComponent(_, _) => {
                            self.sent_updates.remove(&packet_index);
                        }
                        EntityAction::SpawnEntity(global_entity, sent_components) => {
                            let mut component_list =
                                sent_components.expect("sent components not initialized correctly");
                            let entity_record = self.entity_records.get_mut(&global_entity)
                                .expect("created entity does not have a entity_record ... initialization error?");

                            // do we need to delete this now?
                            if self.delayed_entity_deletions.remove(&global_entity) {
                                self.entity_delete(world_record, &global_entity);
                            } else {
                                // set to status of Entity to Created
                                entity_record.status = LocalityStatus::Created;

                                // set status of Components to Created
                                while let Some(component_kind) = component_list.pop() {
                                    if let Some(locality_status) =
                                        entity_record.components.get_mut(&component_kind)
                                    {
                                        *locality_status = LocalityStatus::Created;
                                    } else {
                                        panic!("sent component has not been initialized!");
                                    }
                                }

                                // for any components on this entity that have not yet been created
                                // initiate that now
                                for component_kind in world_record.component_kinds(&global_entity) {
                                    if let Some(locality_status) =
                                        entity_record.components.get(&component_kind)
                                    {
                                        // check if component has been successfully created
                                        // (perhaps through the previous entity_create operation)
                                        if *locality_status == LocalityStatus::Creating {
                                            self.queued_actions.push_back(
                                                EntityAction::InsertComponent(
                                                    global_entity,
                                                    component_kind,
                                                ),
                                            );
                                        }
                                    }
                                }

                                // update delayed entity message structure, to send any Entity
                                // messages that have been waiting
                                self.delayed_entity_messages.add_entity(&global_entity);
                            }
                        }
                        EntityAction::DespawnEntity(global_entity) => {
                            let local_id =
                                self.entity_records.get(&global_entity).unwrap().net_entity;

                            // actually delete the entity from local records
                            self.entity_records.remove(&global_entity);
                            self.delayed_entity_messages.remove_entity(&global_entity);
                            self.local_to_global_entity_map.remove(&local_id);
                            self.entity_generator.recycle_key(&local_id);
                        }
                        EntityAction::InsertComponent(global_entity, component_kind) => {
                            // do we need to delete this now?
                            if self
                                .delayed_component_deletions
                                .remove(&(global_entity, component_kind))
                            {
                                self.component_delete(&global_entity, &component_kind);
                            } else {
                                // we do not need to delete just yet
                                if let Some(entity_record) =
                                    self.entity_records.get_mut(&global_entity)
                                {
                                    if let Some(locality_status) =
                                        entity_record.components.get_mut(&component_kind)
                                    {
                                        *locality_status = LocalityStatus::Created;
                                    } else {
                                        panic!("have not yet initiated component!");
                                    }
                                } else {
                                    panic!("entity does not yet exist for this connection!");
                                }
                            }
                        }
                    }
                }
            }

            for (entity, component_kind) in deleted_components {
                self.component_cleanup(&entity, &component_kind);
            }
        }
    }

    fn entity_delete(&mut self, world_record: &WorldRecord<E, P::Kind>, entity: &E) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            entity_record.status = LocalityStatus::Deleting;

            // Entity deletion IS Component deletion, so update those component records
            // accordingly
            for component_kind in world_record.component_kinds(entity) {
                self.component_cleanup(entity, &component_kind);
            }

            self.queued_actions
                .push_back(EntityAction::DespawnEntity(*entity));
        }
    }

    fn component_delete(&mut self, entity: &E, component_kind: &P::Kind) {
        let entity_record = self
            .entity_records
            .get_mut(entity)
            .expect("attempting to get record of non-existent entity");
        let component_status = entity_record
            .components
            .get_mut(component_kind)
            .expect("attempt to get status of non-existent component of entity");
        *component_status = LocalityStatus::Deleting;

        self.queued_actions
            .push_back(EntityAction::RemoveComponent(*entity, *component_kind));
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> PacketNotifiable
    for EntityManager<P, E, C>
{
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        self.delivered_packets.push_back(packet_index);
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: PacketIndex) {
        if let Some(dropped_actions_list) = self.sent_actions.get_mut(&dropped_packet_index) {
            for dropped_action in dropped_actions_list.drain(..) {
                match dropped_action {
                    // guaranteed delivery actions
                    EntityAction::SpawnEntity { .. }
                    | EntityAction::DespawnEntity(_)
                    | EntityAction::InsertComponent(_, _)
                    | EntityAction::RemoveComponent(_, _) => {
                        self.queued_actions.push_back(dropped_action);
                    }
                    // non-guaranteed delivery actions
                    EntityAction::UpdateComponent(global_entity, component_kind) => {
                        if let Some(diff_mask_map) = self.sent_updates.get(&dropped_packet_index) {
                            let component_index = (global_entity, component_kind);
                            if let Some(diff_mask) = diff_mask_map.get(&component_index) {
                                let mut new_diff_mask = diff_mask.borrow().clone();

                                // walk from dropped packet up to most recently sent packet
                                if dropped_packet_index != self.last_update_packet_index {
                                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                                    while packet_index != self.last_update_packet_index {
                                        if let Some(diff_mask_map) =
                                            self.sent_updates.get(&packet_index)
                                        {
                                            if let Some(next_diff_mask) =
                                                diff_mask_map.get(&component_index)
                                            {
                                                new_diff_mask
                                                    .nand(next_diff_mask.borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                self.diff_handler.or_diff_mask(
                                    &global_entity,
                                    &component_kind,
                                    &new_diff_mask,
                                );
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

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> NetEntityConverter<E>
    for EntityManager<P, E, C>
{
    fn entity_to_net_entity(&self, entity: &E) -> NetEntity {
        return self
            .entity_records
            .get(entity)
            .expect("entity does not exist for this connection!")
            .net_entity;
    }

    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> E {
        return *self
            .local_to_global_entity_map
            .get(net_entity)
            .expect("entity does not exist for this connection!");
    }
}
