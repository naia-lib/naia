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
    message_list_header,
    serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger},
    wrapping_diff, ChannelIndex, DiffMask, EntityConverter, Instant, KeyGenerator, MessageId,
    MessageManager, NetEntity, NetEntityConverter, PacketIndex, PacketNotifiable, Protocolize,
    ReplicateSafe, WorldRefType, MTU_SIZE_BITS,
};

use crate::sequence_list::SequenceList;

use super::{
    entity_action::{EntityAction, EntityActionRecord},
    entity_message_waitlist::EntityMessageWaitlist,
    global_diff_handler::GlobalDiffHandler,
    user_diff_handler::UserDiffHandler,
    world_record::WorldRecord,
};

const RESEND_ACTION_RTT_FACTOR: f32 = 1.5;
const DROP_PACKET_RTT_FACTOR: f32 = 1.5;
const PACKET_RECORD_TTL: Duration = Duration::from_secs(60);

pub type ActionId = MessageId;
pub type SentActions<E, K> =
    HashMap<PacketIndex, (Instant, Vec<(ActionId, EntityActionRecord<K, E>)>)>;

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    // World
    scope_world: HashMap<E, HashSet<P::Kind>>,
    remote_world: HashMap<E, HashSet<P::Kind>>,
    next_action_id: ActionId,
    action_map: HashMap<ActionId, EntityAction<P::Kind, E>>,
    sending_actions: SequenceList<Option<Instant>>,
    sending_entities: HashMap<E, ActionId>,
    sending_components: HashMap<(E, P::Kind), ActionId>,
    next_send_actions: Vec<ActionId>,
    sent_actions: SentActions<E, P::Kind>,

    // Updates
    diff_handler: UserDiffHandler<E, P::Kind>,
    next_send_updates: HashMap<E, HashSet<P::Kind>>,
    sent_updates: HashMap<PacketIndex, (Instant, HashMap<(E, P::Kind), DiffMask>)>,
    last_update_packet_index: PacketIndex,

    // Other
    address: SocketAddr,
    net_entity_generator: KeyGenerator<NetEntity>,
    entity_to_net_entity_map: HashMap<E, NetEntity>,
    net_entity_to_entity_map: HashMap<NetEntity, E>,
    delayed_entity_messages: EntityMessageWaitlist<P, E, C>,
    delivered_packets: VecDeque<PacketIndex>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> EntityManager<P, E, C> {
    /// Create a new NewEntityManager, given the client's address
    pub fn new(
        address: SocketAddr,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E, P::Kind>>>,
    ) -> Self {
        EntityManager {
            // World
            scope_world: HashMap::new(),
            remote_world: HashMap::new(),
            action_map: HashMap::new(),
            sending_actions: SequenceList::new(),
            sending_entities: HashMap::new(),
            sending_components: HashMap::new(),
            next_action_id: 0,
            next_send_actions: Vec::new(),
            sent_actions: HashMap::new(),

            // Update
            diff_handler: UserDiffHandler::new(diff_handler),
            next_send_updates: HashMap::new(),
            sent_updates: HashMap::new(),
            last_update_packet_index: 0,

            // Other
            address,
            net_entity_generator: KeyGenerator::new(),
            net_entity_to_entity_map: HashMap::new(),
            entity_to_net_entity_map: HashMap::new(),
            delayed_entity_messages: EntityMessageWaitlist::new(),
            delivered_packets: VecDeque::new(),
        }
    }

    // World Scope

    pub fn spawn_entity(&mut self, entity: &E) {
        if self.scope_world.contains_key(entity) {
            // do nothing, already in scope
            return;
        }

        self.scope_world.insert(*entity, HashSet::new());

        self.diff_and_generate_actions_entity(entity);

        if !self.entity_to_net_entity_map.contains_key(entity) {
            let new_net_entity = self.net_entity_generator.generate();
            self.entity_to_net_entity_map
                .insert(*entity, new_net_entity);
            self.net_entity_to_entity_map
                .insert(new_net_entity, *entity);
        }
    }

    pub fn despawn_entity(&mut self, entity: &E) {
        if !self.scope_world.contains_key(entity) {
            // do nothing, already not in scope
            return;
        }

        self.scope_world.remove(entity);

        self.diff_and_generate_actions_entity(entity);
    }

    pub fn insert_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.scope_world.contains_key(entity) {
            // possibly this is a bad place to check
            // but currently this is where we check that the scope has the entity
            // before inserting the component
            return;
        }

        let components = self.scope_world.get(entity).unwrap();
        if components.contains(component) {
            // do nothing, already in scope
            return;
        }

        let components = self.scope_world.get_mut(entity).unwrap();
        components.insert(*component);

        self.diff_and_generate_actions_component(entity, component);

        if !self.diff_handler.component_is_registered(entity, component) {
            self.diff_handler
                .register_component(&self.address, entity, component);
        }
    }

    pub fn remove_component(&mut self, entity: &E, component: &P::Kind) {
        let components = self
            .scope_world
            .get(entity)
            .expect("cannot remove component from non-existent entity!");
        if !components.contains(component) {
            // do nothing, already not in scope
            return;
        }

        let components = self.scope_world.get_mut(entity).unwrap();
        components.remove(component);

        self.diff_and_generate_actions_component(entity, component);
    }

    pub fn scope_has_entity(&self, entity: &E) -> bool {
        return self.scope_world.contains_key(entity);
    }

    pub fn has_synced_entity(&self, entity: &E) -> bool {
        return self.scope_world.contains_key(entity) && self.remote_world.contains_key(entity);
    }

    // Messages

    pub fn queue_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entities: Vec<E>,
        channel: C,
        message: &R,
    ) {
        self.delayed_entity_messages
            .queue_message(entities, channel, message.protocol_copy());
    }

    // Writer

    pub fn collect_outgoing_messages(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
        message_manager: &mut MessageManager<P, C>,
    ) {
        self.delayed_entity_messages
            .collect_ready_messages(message_manager);

        self.collect_dropped_update_packets(rtt_millis);
        self.collect_component_updates();

        self.collect_dropped_action_packets();
        self.collect_next_actions(now, rtt_millis);
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.next_send_actions.len() != 0 || self.next_send_updates.len() != 0;
    }

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

    pub fn process_delivered_packets(&mut self) {
        while let Some(packet_index) = self.delivered_packets.pop_front() {
            // Updates
            self.sent_updates.remove(&packet_index);

            // Actions
            if let Some((_, action_list)) = self.sent_actions.remove(&packet_index) {
                for (action_id, action) in action_list {
                    match action {
                        EntityActionRecord::SpawnEntity(entity, components) => {
                            self.remote_spawned_entity(&action_id, &entity, components);
                        }
                        EntityActionRecord::DespawnEntity(entity) => {
                            self.remote_despawned_entity(&action_id, &entity);
                        }
                        EntityActionRecord::InsertComponent(entity, component) => {
                            self.remote_inserted_component(&action_id, &entity, &component);
                        }
                        EntityActionRecord::RemoveComponent(entity, component) => {
                            self.remote_removed_component(&action_id, &entity, &component);
                        }
                        EntityActionRecord::Noop => {
                            self.remote_received_noop(&action_id);
                        }
                    }
                }
            }
        }
    }

    // Private methods

    // diffing world

    fn diff_and_generate_actions_entity(&mut self, entity: &E) {
        let scope_has_entity = self.scope_world.contains_key(entity);
        let remote_has_entity = self.remote_world.contains_key(entity);

        if scope_has_entity == remote_has_entity {
            // already synced up
            // remove sending action
            if self.sending_entities.contains_key(entity) {
                self.cancel_sending_entity(entity);
            }

            return;
        }

        // check whether change is already sending
        let new_action = match scope_has_entity {
            true => EntityAction::SpawnEntity(*entity),
            false => EntityAction::DespawnEntity(*entity),
        };

        if self.sending_entities.contains_key(entity) {
            let old_action_id = self.sending_entities.get(entity).unwrap();
            let old_action = self.action_map.get(old_action_id).unwrap();
            if *old_action == new_action {
                // change is already in progress
                return;
            }

            self.cancel_sending_entity(entity);
        }

        let new_action_id = self.new_action_id();
        self.action_map.insert(new_action_id, new_action);
        self.sending_actions.push_from_back(new_action_id, None);
        self.sending_entities.insert(*entity, new_action_id);
    }

    fn diff_and_generate_actions_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.remote_world.contains_key(entity) {
            // will update entity with correct components after it spawns
            // do not collect actions here
            return;
        }

        let scope_has_component = self
            .scope_world
            .get(entity)
            .expect("cannot collect component actions from non-existent entity!")
            .contains(component);
        let remote_has_component = self.remote_world.get(entity).unwrap().contains(component);

        if scope_has_component == remote_has_component {
            // already synced up

            // remove sending action
            if self.sending_components.contains_key(&(*entity, *component)) {
                self.cancel_sending_component(entity, component);
            }

            return;
        }

        // check whether change is already sending
        let new_action = match scope_has_component {
            true => EntityAction::InsertComponent(*entity, *component),
            false => EntityAction::RemoveComponent(*entity, *component),
        };

        if let Some(old_action_id) = self.sending_components.get(&(*entity, *component)) {
            let old_action = self.action_map.get(old_action_id).unwrap();
            if *old_action == new_action {
                // action is the same, no need to re-generate it
                return;
            }

            self.cancel_sending_component(entity, component);
        }

        let new_action_id = self.new_action_id();
        self.action_map.insert(new_action_id, new_action);
        self.sending_actions.push_from_back(new_action_id, None);
        self.sending_components
            .insert((*entity, *component), new_action_id);
    }

    // Syncing Scope -> Remote, creating Actions

    fn new_action_id(&mut self) -> ActionId {
        let output = self.next_action_id;
        self.next_action_id = self.next_action_id.wrapping_add(1);
        output
    }

    fn cancel_sending_entity(&mut self, entity: &E) {
        // remove currently sending action
        let action_id = self.sending_entities.remove(entity).unwrap();

        // replace action in record with noop
        let action = self.action_map.get_mut(&action_id).unwrap();
        *action = EntityAction::Noop;
    }

    fn cancel_sending_component(&mut self, entity: &E, component: &P::Kind) {
        // remove currently sending action
        let action_id = self
            .sending_components
            .remove(&(*entity, *component))
            .unwrap();

        // replace action in record with noop
        let action = self.action_map.get_mut(&action_id).unwrap();
        *action = EntityAction::Noop;
    }

    // Erasing

    fn remove_action_entity(&mut self, action_id: &ActionId, entity: &E) {
        self.action_map.remove(action_id);
        self.sending_actions.remove_from_front(action_id);

        let mut remove = false;
        if let Some(current_action_id) = self.sending_entities.get(entity) {
            if *action_id == *current_action_id {
                remove = true;
            }
        }
        if remove {
            self.sending_entities.remove(entity);
        }
    }

    fn remove_action_component(&mut self, action_id: &ActionId, entity: &E, component: &P::Kind) {
        self.action_map.remove(action_id);
        self.sending_actions.remove_from_front(action_id);

        let mut remove = false;
        if let Some(current_action_id) = self.sending_components.get(&(*entity, *component)) {
            if *action_id == *current_action_id {
                remove = true;
            }
        }
        if remove {
            self.sending_components.remove(&(*entity, *component));
        }
    }

    // Processing delivered actions

    fn remote_spawned_entity(
        &mut self,
        action_id: &ActionId,
        entity: &E,
        components: Vec<P::Kind>,
    ) {
        if !self.action_map.contains_key(action_id) {
            // action has already been delivered before, ignore
            return;
        }
        self.remove_action_entity(action_id, entity);

        if self.remote_world.contains_key(entity) {
            // who knows how this updated already .. best do nothing?
        } else {
            let mut component_set = HashSet::new();
            for component in &components {
                component_set.insert(*component);
            }
            self.remote_world.insert(*entity, component_set);

            if !self.scope_world.contains_key(entity) {
                // entity has despawned again... collect updates
                self.diff_and_generate_actions_entity(entity);
            } else {
                self.delayed_entity_messages.add_entity(entity);

                let mut scope_components = Vec::new();
                {
                    let scope_component_set = self.scope_world.get(entity).unwrap();
                    for component in scope_component_set {
                        scope_components.push(*component);
                    }
                }
                for scope_component in scope_components {
                    self.diff_and_generate_actions_component(entity, &scope_component);
                }
            }
        }
    }

    fn remote_despawned_entity(&mut self, action_id: &ActionId, entity: &E) {
        if !self.action_map.contains_key(action_id) {
            // action has already been delivered before, ignore
            return;
        }
        self.remove_action_entity(action_id, entity);

        if !self.remote_world.contains_key(entity) {
            // who knows how this updated already .. best do nothing?
        } else {
            self.remote_world.remove(entity);
            self.delayed_entity_messages.remove_entity(entity);
            self.diff_and_generate_actions_entity(entity);

            // if we are truly done with this entity... remove

            if !self.scope_world.contains_key(entity) && !self.sending_entities.contains_key(entity)
            {
                // delete net entity map
                if self.entity_to_net_entity_map.contains_key(entity) {
                    let net_entity = self.entity_to_net_entity_map.remove(entity).unwrap();
                    self.net_entity_to_entity_map.remove(&net_entity);
                }
            }
        }
    }

    fn remote_inserted_component(&mut self, action_id: &ActionId, entity: &E, component: &P::Kind) {
        if !self.action_map.contains_key(action_id) {
            // action has already been delivered before, ignore
            return;
        }
        self.remove_action_component(action_id, entity, component);

        if !self.remote_world.contains_key(entity) {
            // entity despawned on the remote... very odd
            self.diff_and_generate_actions_entity(entity);
            return;
        }

        let remote_component_set = self.remote_world.get_mut(entity).unwrap();
        remote_component_set.insert(*component);

        if !self.scope_world.contains_key(entity) {
            // entity despawned in the scope... very odd
            self.diff_and_generate_actions_entity(entity);
            return;
        }

        self.diff_and_generate_actions_component(entity, component);
    }

    fn remote_removed_component(&mut self, action_id: &ActionId, entity: &E, component: &P::Kind) {
        if !self.action_map.contains_key(action_id) {
            // action has already been delivered before, ignore
            return;
        }
        self.remove_action_component(action_id, entity, component);

        if !self.remote_world.contains_key(entity) {
            // entity despawned on the remote... very odd
            self.diff_and_generate_actions_entity(entity);
            return;
        }

        let remote_component_set = self.remote_world.get_mut(entity).unwrap();
        remote_component_set.remove(component);

        if !self.scope_world.contains_key(entity) {
            // entity despawned in the scope... very odd
            self.diff_and_generate_actions_entity(entity);
            return;
        }

        self.diff_and_generate_actions_component(entity, component);

        // if we are truly done with this component .. deregister
        // deregister component from diff handler if applicable
        let scope_has_component = match self.scope_world.get(entity) {
            Some(scope_components) => scope_components.contains(component),
            None => false,
        };
        let sending_has_component = self.sending_components.contains_key(&(*entity, *component));
        if !scope_has_component && !sending_has_component {
            if self.diff_handler.component_is_registered(entity, component) {
                self.diff_handler.deregister_component(entity, component);
            }
        }
    }

    fn remote_received_noop(&mut self, action_id: &ActionId) {
        if !self.action_map.contains_key(action_id) {
            // action has already been delivered before, ignore
            return;
        }
        self.action_map.remove(action_id);
        self.sending_actions.remove_from_front(action_id);
    }

    // Collecting

    fn collect_dropped_action_packets(&mut self) {
        let mut dropped_packets = Vec::new();
        for (packet_index, (time_sent, _)) in &self.sent_actions {
            if time_sent.elapsed() > PACKET_RECORD_TTL {
                dropped_packets.push(*packet_index);
            }
        }

        for packet_index in dropped_packets {
            self.sent_actions.remove(&packet_index);
        }
    }

    fn collect_next_actions(&mut self, now: &Instant, rtt_millis: &f32) {
        // TODO: make self.sending_actions an ascending list so that iteration order is
        // from oldest -> newest action id

        let resend_duration = Duration::from_millis((RESEND_ACTION_RTT_FACTOR * rtt_millis) as u64);

        // go through sending actions, if we haven't sent in a while, add message to
        // outgoing queue
        for (action_id, last_sent_opt) in self.sending_actions.iter_mut() {
            // check whether we should send outgoing actions in the next packet
            let mut should_send = false;

            if let Some(last_sent) = last_sent_opt {
                if last_sent.elapsed() > resend_duration {
                    should_send = true;
                }
            } else {
                should_send = true;
            }

            if !should_send {
                continue;
            }

            // put action into outgoing queue
            self.next_send_actions.push(*action_id);

            *last_sent_opt = Some(now.clone());
        }
    }

    fn collect_dropped_update_packets(&mut self, rtt_millis: &f32) {
        let drop_duration = Duration::from_millis((DROP_PACKET_RTT_FACTOR * rtt_millis) as u64);

        {
            let mut dropped_packets = Vec::new();
            for (packet_index, (time_sent, _)) in &self.sent_updates {
                if time_sent.elapsed() > drop_duration {
                    dropped_packets.push(*packet_index);
                }
            }

            for packet_index in dropped_packets {
                self.dropped_update_cleanup(packet_index);
            }
        }
    }

    fn dropped_update_cleanup(&mut self, dropped_packet_index: PacketIndex) {
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

    fn collect_component_updates(&mut self) {
        for (global_entity, component_set) in self.remote_world.iter() {
            if !self.scope_world.contains_key(global_entity) {
                // this entity is about to be deleted, no sense sending updates in the meantime
                continue;
            }
            for component_kind in component_set.iter() {
                let scope_components = self.scope_world.get(global_entity).unwrap();
                if !scope_components.contains(component_kind) {
                    // this component is about to be deleted, no sense sending updates in the
                    // meantime
                    continue;
                }

                if self
                    .diff_handler
                    .diff_mask_is_clear(global_entity, component_kind)
                {
                    // no updates detected, do nothing
                    continue;
                }

                if !self.next_send_updates.contains_key(global_entity) {
                    self.next_send_updates
                        .insert(*global_entity, HashSet::new());
                }
                let send_component_set = self.next_send_updates.get_mut(global_entity).unwrap();
                send_component_set.insert(*component_kind);
            }
        }
    }

    // Writing actions

    fn write_action_id(
        bit_writer: &mut dyn BitWrite,
        last_id_opt: &mut Option<ActionId>,
        current_id: &ActionId,
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

    fn can_write_action<W: WorldRefType<P, E>>(
        &self,
        world: &W,
        action: &EntityAction<P::Kind, E>,
    ) -> bool {
        match action {
            EntityAction::InsertComponent(global_entity, component_kind) => {
                if !world.has_component_of_kind(global_entity, component_kind) {
                    return false;
                }
            }
            _ => {}
        }

        return true;
    }

    fn write_actions<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        let mut message_count = 0;

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
            let next_send_actions_len = self.next_send_actions.len();
            let mut last_written_id: Option<ActionId> = None;

            for action_index in 0..next_send_actions_len {
                self.write_action(
                    world,
                    world_record,
                    packet_index,
                    &mut counter,
                    action_index,
                    &mut last_written_id,
                    false,
                );
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }
            }
        }

        // Write header
        message_list_header::write(writer, message_count as u64);

        if !self.sent_actions.contains_key(&packet_index) {
            self.sent_actions
                .insert(*packet_index, (now.clone(), Vec::new()));
        }

        // Actions
        {
            let mut last_written_id: Option<ActionId> = None;

            // Write messages
            for action_index in 0..message_count {
                self.write_action(
                    world,
                    world_record,
                    packet_index,
                    writer,
                    action_index,
                    &mut last_written_id,
                    true,
                );
            }

            // Pop messages
            self.next_send_actions.drain(..message_count);
        }
    }

    fn write_action<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
        packet_index: &PacketIndex,
        bit_writer: &mut dyn BitWrite,
        action_index: usize,
        last_written_id: &mut Option<ActionId>,
        is_writing: bool,
    ) {
        let action_id = self.next_send_actions.get(action_index).unwrap();
        let action = self.action_map.get(action_id).unwrap();

        if !self.can_write_action(world, action) {
            return;
        }

        // write EntityAction type
        action.as_type().ser(bit_writer);

        // write message id
        Self::write_action_id(bit_writer, last_written_id, action_id);

        match action {
            EntityAction::SpawnEntity(entity) => {
                // write net entity
                self.entity_to_net_entity_map
                    .get(entity)
                    .unwrap()
                    .ser(bit_writer);

                // get component list
                let component_kinds = world_record.component_kinds(&entity);

                // write number of components
                let components_num =
                    UnsignedVariableInteger::<3>::new(component_kinds.len() as i128);
                components_num.ser(bit_writer);

                for component_kind in &component_kinds {
                    // write kind
                    component_kind.ser(bit_writer);

                    // write payload
                    let component = world
                        .component_of_kind(entity, &component_kind)
                        .expect("Component does not exist in World");

                    {
                        let converter = EntityConverter::new(world_record, self);
                        component.write(bit_writer, &converter);
                    }

                    // only clear diff mask if we are actually writing the packet
                    if is_writing {
                        self.diff_handler.clear_diff_mask(entity, &component_kind);
                    }
                }

                // if we are writing to this packet, add it to record
                if is_writing {
                    Self::record_action_written(
                        &mut self.sent_actions,
                        packet_index,
                        action_id,
                        EntityActionRecord::SpawnEntity(*entity, component_kinds),
                    );
                }
            }
            EntityAction::DespawnEntity(entity) => {
                // write net entity
                self.entity_to_net_entity_map
                    .get(entity)
                    .unwrap()
                    .ser(bit_writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    Self::record_action_written(
                        &mut self.sent_actions,
                        packet_index,
                        action_id,
                        EntityActionRecord::DespawnEntity(*entity),
                    );
                }
            }
            EntityAction::InsertComponent(entity, component) => {
                // write net entity
                self.entity_to_net_entity_map
                    .get(entity)
                    .unwrap()
                    .ser(bit_writer);

                // write component kind
                component.ser(bit_writer);

                // write component payload
                let component_ref = world
                    .component_of_kind(entity, component)
                    .expect("Component does not exist in World");

                {
                    let converter = EntityConverter::new(world_record, self);
                    component_ref.write(bit_writer, &converter);
                }

                // if we are actually writing this packet
                if is_writing {
                    // clear the component's diff mask
                    self.diff_handler.clear_diff_mask(&entity, component);

                    // add it to action record
                    Self::record_action_written(
                        &mut self.sent_actions,
                        packet_index,
                        action_id,
                        EntityActionRecord::InsertComponent(*entity, *component),
                    );
                }
            }
            EntityAction::RemoveComponent(entity, component) => {
                // write net entity
                self.entity_to_net_entity_map
                    .get(entity)
                    .unwrap()
                    .ser(bit_writer);

                // write component kind
                component.ser(bit_writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    Self::record_action_written(
                        &mut self.sent_actions,
                        packet_index,
                        action_id,
                        EntityActionRecord::RemoveComponent(*entity, *component),
                    );
                }
            }
            EntityAction::Noop => {
                // no need to write anything here

                // if we are writing to this packet, add it to record
                if is_writing {
                    Self::record_action_written(
                        &mut self.sent_actions,
                        packet_index,
                        action_id,
                        EntityActionRecord::Noop,
                    );
                }
            }
        }
    }

    fn record_action_written(
        sent_actions: &mut SentActions<E, P::Kind>,
        packet_index: &PacketIndex,
        action_id: &ActionId,
        action_record: EntityActionRecord<P::Kind, E>,
    ) {
        let (_, sent_actions_list) = sent_actions.get_mut(&packet_index).unwrap();
        sent_actions_list.push((*action_id, action_record));
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
            let all_update_entities: Vec<E> = self.next_send_updates.keys().map(|e| *e).collect();

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
                self.next_send_updates
                    .remove(global_entity)
                    .expect("should be an update available to pop"),
            );
        }
        let component_set = {
            if is_writing {
                update_holder.as_ref().unwrap()
            } else {
                self.next_send_updates.get(global_entity).as_ref().unwrap()
            }
        };

        // write net entity
        self.entity_to_net_entity_map
            .get(global_entity)
            .unwrap()
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
        return *self
            .entity_to_net_entity_map
            .get(entity)
            .expect("entity does not exist for this connection!");
    }

    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> E {
        return *self
            .net_entity_to_entity_map
            .get(net_entity)
            .expect("entity does not exist for this connection!");
    }
}
