use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use naia_shared::{
    message_list_header, sequence_greater_than,
    serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger},
    wrapping_diff, ChannelIndex, DiffMask, EntityConverter, Instant, KeyGenerator, MessageId,
    MessageManager, NetEntity, NetEntityConverter, PacketIndex, PacketNotifiable, Protocolize,
    ReplicateSafe, WorldRefType, MTU_SIZE_BITS,
};

use crate::world_record::WorldRecord;

use super::{
    entity_action::EntityAction, entity_message_waitlist::EntityMessageWaitlist,
    global_diff_handler::GlobalDiffHandler, local_entity_record::LocalEntityRecord,
    locality_status::LocalityStatus, user_diff_handler::UserDiffHandler,
};

const DROP_PACKET_RTT_FACTOR: f32 = 1.5;

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
    queued_actions: QueuedEntityActions<P, E>,
    queued_updates: HashMap<E, HashSet<P::Kind>>,
    sent_actions: HashMap<PacketIndex, (Instant, Vec<(MessageId, EntityAction<P, E>)>)>,
    sent_updates: HashMap<PacketIndex, (Instant, HashMap<(E, P::Kind), DiffMask>)>,
    last_update_packet_index: PacketIndex,
    delivered_packets: VecDeque<PacketIndex>,
    // reliable_action_sender: ReliableSender<P>,
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
            queued_actions: QueuedEntityActions::new(),
            queued_updates: HashMap::new(),
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
                .push_new(EntityAction::SpawnEntity(*global_entity, None));
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

    pub fn collect_outgoing_messages(
        &mut self,
        rtt_millis: &f32,
        message_manager: &mut MessageManager<P, C>,
    ) {
        self.collect_dropped_messages(rtt_millis);
        self.delayed_entity_messages
            .collect_ready_messages(message_manager);
        self.collect_component_updates();
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
                    .push_new(EntityAction::InsertComponent(*entity, *component_kind));
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

    pub fn write_all<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        self.write_actions(&now, writer, packet_index, world, world_record);
        self.write_updates(&now, writer, packet_index, world, world_record);
    }

    // Ect..

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_actions.len() != 0 || self.queued_updates.len() != 0;
    }

    pub fn process_delivered_packets(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        while let Some(packet_index) = self.delivered_packets.pop_front() {
            self.sent_updates.remove(&packet_index);

            let mut deleted_components: Vec<(E, P::Kind)> = Vec::new();

            if let Some((_, delivered_actions_list)) = self.sent_actions.remove(&packet_index) {
                for (_, delivered_action) in delivered_actions_list.into_iter() {
                    match delivered_action {
                        EntityAction::RemoveComponent(global_entity, component_kind) => {
                            deleted_components.push((global_entity, component_kind));
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
                                            self.queued_actions.push_new(
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

    // Private methods

    fn write_actions<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
                return;
            }

            let mut counter = BitCounter::new();
            message_list_header::write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
                return;
            }

            // Find how many messages will fit into the packet
            let queued_actions_len = self.queued_actions.len();
            let mut last_written_id: Option<MessageId> = None;

            for action_index in 0..queued_actions_len {
                self.write_action(
                    world,
                    world_record,
                    packet_index,
                    &mut counter,
                    Some(action_index),
                    &mut last_written_id,
                );
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }
            }
        }

        // Write header
        message_list_header::write(writer, message_count);

        if !self.sent_actions.contains_key(&packet_index) {
            self.sent_actions
                .insert(*packet_index, (now.clone(), Vec::new()));
        }

        // Actions
        {
            let mut last_written_id: Option<MessageId> = None;

            for _ in 0..message_count {
                // Pop message

                // Write message
                self.write_action(
                    world,
                    world_record,
                    packet_index,
                    writer,
                    None,
                    &mut last_written_id,
                );
            }
        }
    }

    fn write_message_id(
        bit_writer: &mut dyn BitWrite,
        last_id_opt: &mut Option<MessageId>,
        current_id: &MessageId,
    ) {
        if let Some(last_id) = last_id_opt {
            // write diff
            let id_diff = wrapping_diff(*last_id, *current_id);
            let id_diff_encoded = UnsignedVariableInteger::<3>::new(id_diff);
            id_diff_encoded.ser(bit_writer);
        } else {
            // write message id
            current_id.ser(bit_writer);
        }
        *last_id_opt = Some(*current_id);
    }

    fn write_action<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
        packet_index: &PacketIndex,
        bit_writer: &mut dyn BitWrite,
        action_index: Option<usize>,
        last_written_id: &mut Option<MessageId>,
    ) {
        let is_writing: bool = action_index.is_none();
        let mut action_holder: Option<(MessageId, EntityAction<P, E>)> = None;
        if is_writing {
            action_holder = Some(
                self.queued_actions
                    .pop()
                    .expect("should be an action available to pop"),
            );
        }
        let (action_id, action) = {
            if is_writing {
                action_holder.as_ref().unwrap()
            } else {
                let open_action_index = action_index.unwrap();
                self.queued_actions.get(open_action_index).as_ref().unwrap()
            }
        };

        // write EntityAction type
        action.as_type().ser(bit_writer);

        // write message id
        Self::write_message_id(bit_writer, last_written_id, action_id);

        match action {
            EntityAction::SpawnEntity(global_entity, _) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(bit_writer);

                // get component list
                let mut component_kinds = Vec::new();
                for component_kind in world_record.component_kinds(&global_entity) {
                    component_kinds.push(component_kind);
                }

                // write number of components
                let components_num =
                    UnsignedVariableInteger::<3>::new(component_kinds.len() as i128);
                components_num.ser(bit_writer);

                for component_kind in &component_kinds {
                    // write kind
                    component_kind.ser(bit_writer);

                    // write payload
                    let component = world
                        .component_of_kind(global_entity, &component_kind)
                        .expect("Component does not exist in World");

                    {
                        let converter = EntityConverter::new(world_record, self);
                        component.write(bit_writer, &converter);
                    }

                    // only clear diff mask if we are actually writing the packet
                    if is_writing {
                        self.diff_handler
                            .clear_diff_mask(global_entity, &component_kind);
                    }
                }

                // write to record, if we are writing to this packet
                if is_writing {
                    let (_, sent_actions_list) = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push((
                        *action_id,
                        EntityAction::SpawnEntity(*global_entity, Some(component_kinds)),
                    ));
                }
            }
            EntityAction::DespawnEntity(global_entity) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(bit_writer);

                // write to record, if we are writing to this packet
                if is_writing {
                    let (_, sent_actions_list) = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list
                        .push((*action_id, EntityAction::DespawnEntity(*global_entity)));
                }
            }
            EntityAction::InsertComponent(global_entity, component_kind) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(bit_writer);

                // write component kind
                component_kind.ser(bit_writer);

                // write component payload
                let component = world
                    .component_of_kind(global_entity, component_kind)
                    .expect("Component does not exist in World");

                {
                    let converter = EntityConverter::new(world_record, self);
                    component.write(bit_writer, &converter);
                }

                // if we are actually writing this packet
                if is_writing {
                    // clear the component's diff mask
                    self.diff_handler
                        .clear_diff_mask(&global_entity, component_kind);

                    // write to record
                    let (_, sent_actions_list) = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push((
                        *action_id,
                        EntityAction::InsertComponent(*global_entity, *component_kind),
                    ));
                }
            }
            EntityAction::RemoveComponent(global_entity, component_kind) => {
                // write local entity
                let net_entity = self.entity_records.get(global_entity).unwrap().net_entity;
                net_entity.ser(bit_writer);

                // write component kind
                component_kind.ser(bit_writer);

                // if we are writing to this packet
                if is_writing {
                    // write to record
                    let (_, sent_actions_list) = self.sent_actions.get_mut(&packet_index).unwrap();
                    sent_actions_list.push((
                        *action_id,
                        EntityAction::RemoveComponent(*global_entity, *component_kind),
                    ));
                }
            }
        }
    }

    fn write_updates<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        let mut update_entities: Vec<E> = Vec::new();

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
                return;
            }

            let mut counter = BitCounter::new();
            message_list_header::write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
                return;
            }

            // Find how many messages will fit into the packet
            let all_update_entities: Vec<E> = self.queued_updates.keys().map(|e| *e).collect();

            for update_entity in all_update_entities {
                self.write_update(
                    world,
                    world_record,
                    packet_index,
                    &mut counter,
                    &update_entity,
                    false,
                );
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    update_entities.push(update_entity);
                } else {
                    break;
                }
            }
        }

        // Write header
        message_list_header::write(writer, update_entities.len() as u16);

        if !self.sent_updates.contains_key(&packet_index) {
            self.sent_updates
                .insert(*packet_index, (now.clone(), HashMap::new()));
        }

        // Actions
        {
            for entity in update_entities {
                // Pop message

                // Write message
                self.write_update(world, world_record, packet_index, writer, &entity, true);
            }
        }
    }

    fn write_update<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
        packet_index: &PacketIndex,
        bit_writer: &mut dyn BitWrite,
        global_entity: &E,
        is_writing: bool,
    ) {
        let mut update_holder: Option<HashSet<P::Kind>> = None;
        if is_writing {
            update_holder = Some(
                self.queued_updates
                    .remove(global_entity)
                    .expect("should be an update available to pop"),
            );
        }
        let component_set = {
            if is_writing {
                update_holder.as_ref().unwrap()
            } else {
                self.queued_updates.get(global_entity).as_ref().unwrap()
            }
        };

        // write local entity
        self.entity_records
            .get(global_entity)
            .unwrap()
            .net_entity
            .ser(bit_writer);

        // write number of components
        UnsignedVariableInteger::<3>::new(component_set.len() as u64).ser(bit_writer);

        for component_kind in component_set {
            // write component kind
            component_kind.ser(bit_writer);

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
                    .write_partial(&diff_mask, bit_writer, &converter);
            }

            ////////
            if is_writing {
                // place diff mask in a special transmission record - like map
                self.last_update_packet_index = *packet_index;

                let (_, sent_updates_map) = self.sent_updates.get_mut(packet_index).unwrap();
                sent_updates_map.insert((*global_entity, *component_kind), diff_mask);

                // having copied the diff mask for this update, clear the component
                self.diff_handler
                    .clear_diff_mask(global_entity, component_kind);
            }
        }
    }

    fn collect_dropped_messages(&mut self, rtt_millis: &f32) {
        let drop_duration = Duration::from_millis((DROP_PACKET_RTT_FACTOR * rtt_millis) as u64);

        {
            let mut dropped_packets = Vec::new();
            for (packet_index, (time_sent, _)) in &self.sent_actions {
                if time_sent.elapsed() > drop_duration {
                    dropped_packets.push(*packet_index);
                }
            }

            for packet_index in dropped_packets {
                self.notify_action_dropped(packet_index);
            }
        }

        {
            let mut dropped_packets = Vec::new();
            for (packet_index, (time_sent, _)) in &self.sent_updates {
                if time_sent.elapsed() > drop_duration {
                    dropped_packets.push(*packet_index);
                }
            }

            for packet_index in dropped_packets {
                self.notify_update_dropped(packet_index);
            }
        }
    }

    fn collect_component_updates(&mut self) {
        for (global_entity, entity_record) in self.entity_records.iter() {
            for (component_kind, locality_status) in entity_record.components.iter() {
                if *locality_status == LocalityStatus::Created
                    && !self
                        .diff_handler
                        .diff_mask_is_clear(global_entity, component_kind)
                {
                    if !self.queued_updates.contains_key(global_entity) {
                        self.queued_updates.insert(*global_entity, HashSet::new());
                    }
                    let component_set = self.queued_updates.get_mut(global_entity).unwrap();
                    component_set.insert(*component_kind);
                }
            }
        }
    }

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

    fn entity_delete(&mut self, world_record: &WorldRecord<E, P::Kind>, entity: &E) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            entity_record.status = LocalityStatus::Deleting;

            // Entity deletion IS Component deletion, so update those component records
            // accordingly
            for component_kind in world_record.component_kinds(entity) {
                self.component_cleanup(entity, &component_kind);
            }

            self.queued_actions
                .push_new(EntityAction::DespawnEntity(*entity));
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
            .push_new(EntityAction::RemoveComponent(*entity, *component_kind));
    }

    fn notify_action_dropped(&mut self, dropped_packet_index: PacketIndex) {
        if let Some((_, mut dropped_actions_list)) = self.sent_actions.remove(&dropped_packet_index)
        {
            for (action_id, dropped_action) in dropped_actions_list.drain(..) {
                match dropped_action {
                    // guaranteed delivery actions
                    EntityAction::SpawnEntity { .. }
                    | EntityAction::DespawnEntity(_)
                    | EntityAction::InsertComponent(_, _)
                    | EntityAction::RemoveComponent(_, _) => {
                        self.queued_actions.push_old(action_id, dropped_action);
                    }
                }
            }
        }
    }

    fn notify_update_dropped(&mut self, dropped_packet_index: PacketIndex) {
        if let Some((_, diff_mask_map)) = self.sent_updates.remove(&dropped_packet_index) {
            // non-guaranteed delivery actions
            for (component_index, diff_mask) in &diff_mask_map {
                let (global_entity, component_kind) = component_index;
                let mut new_diff_mask = diff_mask.borrow().clone();

                // walk from dropped packet up to most recently sent packet
                if dropped_packet_index == self.last_update_packet_index {
                    continue;
                }

                let mut packet_index = dropped_packet_index.wrapping_add(1);
                while packet_index != self.last_update_packet_index {
                    if let Some((_, diff_mask_map)) = self.sent_updates.get(&packet_index) {
                        if let Some(next_diff_mask) = diff_mask_map.get(&component_index) {
                            new_diff_mask.nand(next_diff_mask.borrow().borrow());
                        }
                    }

                    packet_index = packet_index.wrapping_add(1);
                }

                self.diff_handler
                    .or_diff_mask(&global_entity, &component_kind, &new_diff_mask);
            }
        }
    }
}

// QueuedEntityActions
pub struct QueuedEntityActions<P: Protocolize, E: Copy + Eq + Hash> {
    next_send_message_id: MessageId,
    list: VecDeque<(MessageId, EntityAction<P, E>)>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> QueuedEntityActions<P, E> {
    pub fn new() -> Self {
        Self {
            next_send_message_id: 0,
            list: VecDeque::new(),
        }
    }

    pub fn push_new(&mut self, action: EntityAction<P, E>) {
        self.push_ordered(self.next_send_message_id, action);
        self.next_send_message_id = self.next_send_message_id.wrapping_add(1);
    }

    pub fn push_old(&mut self, message_id: MessageId, action: EntityAction<P, E>) {
        self.push_ordered(message_id, action);
    }

    pub fn pop(&mut self) -> Option<(MessageId, EntityAction<P, E>)> {
        return self.list.pop_front();
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn get(&self, index: usize) -> Option<&(MessageId, EntityAction<P, E>)> {
        return self.list.get(index);
    }

    fn push_ordered(&mut self, message_id: MessageId, action: EntityAction<P, E>) {
        let mut index = 0;

        loop {
            if index < self.list.len() {
                let (old_message_id, _) = self.list.get(index).unwrap();
                if *old_message_id == message_id {
                    panic!("should never get here, how can duplicate actions get added to this?");
                }
                if sequence_greater_than(*old_message_id, message_id) {
                    self.list.insert(index, (message_id, action));
                    break;
                }
            } else {
                self.list.push_back((message_id, action));
                break;
            }

            index += 1;
        }
    }
}

// PacketNotifiable
impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> PacketNotifiable
    for EntityManager<P, E, C>
{
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        self.delivered_packets.push_back(packet_index);
    }
}

// NetEntityConverter
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
