use std::{
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
    wrapping_diff, ChannelIndex, DiffMask, EntityAction, EntityActionType, EntityConverter,
    Instant, MessageId, MessageManager, NetEntity, NetEntityConverter, PacketIndex,
    PacketNotifiable, Protocolize, ReplicateSafe, WorldRefType, MTU_SIZE_BITS,
};

use crate::sequence_list::SequenceList;

use super::{
    entity_action_event::EntityActionEvent, global_diff_handler::GlobalDiffHandler,
    world_channel::WorldChannel, world_record::WorldRecord,
};

const DROP_UPDATE_RTT_FACTOR: f32 = 1.5;
const ACTION_RECORD_TTL: Duration = Duration::from_secs(60);

pub type ActionId = MessageId;

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex> {
    // World
    world_channel: WorldChannel<P, E, C>,
    next_send_actions: VecDeque<(ActionId, EntityActionEvent<E, P::Kind>)>,
    #[allow(clippy::type_complexity)]
    sent_action_packets: SequenceList<(Instant, Vec<(ActionId, EntityAction<E, P::Kind>)>)>,

    // Updates
    next_send_updates: HashMap<E, HashSet<P::Kind>>,
    #[allow(clippy::type_complexity)]
    sent_updates: HashMap<PacketIndex, (Instant, HashMap<(E, P::Kind), DiffMask>)>,
    last_update_packet_index: PacketIndex,
}

impl<P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex> EntityManager<P, E, C> {
    /// Create a new NewEntityManager, given the client's address
    pub fn new(
        address: SocketAddr,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E, P::Kind>>>,
    ) -> Self {
        EntityManager {
            // World
            world_channel: WorldChannel::new(address, diff_handler),
            next_send_actions: VecDeque::new(),
            sent_action_packets: SequenceList::new(),

            // Update
            next_send_updates: HashMap::new(),
            sent_updates: HashMap::new(),
            last_update_packet_index: 0,
        }
    }

    // World Scope

    pub fn spawn_entity(&mut self, entity: &E) {
        self.world_channel.host_spawn_entity(entity);
    }

    pub fn despawn_entity(&mut self, entity: &E) {
        self.world_channel.host_despawn_entity(entity);
    }

    pub fn insert_component(&mut self, entity: &E, component: &P::Kind) {
        self.world_channel.host_insert_component(entity, component);
    }

    pub fn remove_component(&mut self, entity: &E, component: &P::Kind) {
        self.world_channel.host_remove_component(entity, component);
    }

    pub fn scope_has_entity(&self, entity: &E) -> bool {
        self.world_channel.host_has_entity(entity)
    }

    pub fn entity_channel_is_open(&self, entity: &E) -> bool {
        self.world_channel.entity_channel_is_open(entity)
    }

    // Messages

    pub fn queue_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entities: Vec<E>,
        channel: C,
        message: &R,
    ) {
        self.world_channel.delayed_entity_messages.queue_message(
            entities,
            channel,
            message.protocol_copy(),
        );
    }

    // Writer

    pub fn collect_outgoing_messages(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
        message_manager: &mut MessageManager<P, C>,
    ) {
        self.world_channel
            .delayed_entity_messages
            .collect_ready_messages(message_manager);

        self.collect_dropped_update_packets(rtt_millis);

        self.collect_dropped_action_packets();
        self.collect_next_actions(now, rtt_millis);

        self.collect_component_updates();
    }

    pub fn has_outgoing_messages(&self) -> bool {
        !self.next_send_actions.is_empty() || !self.next_send_updates.is_empty()
    }

    pub fn write_all<W: WorldRefType<P, E>>(
        &mut self,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E, <P as Protocolize>::Kind>,
    ) {
        self.write_updates(now, writer, packet_index, world, world_record);
        self.write_actions(now, writer, packet_index, world, world_record);
    }

    // Collecting

    fn collect_dropped_action_packets(&mut self) {
        let mut pop = false;

        loop {
            if let Some((_, (time_sent, _))) = self.sent_action_packets.front() {
                if time_sent.elapsed() > ACTION_RECORD_TTL {
                    pop = true;
                }
            } else {
                return;
            }
            if pop {
                self.sent_action_packets.pop_front();
            } else {
                return;
            }
        }
    }

    fn collect_next_actions(&mut self, now: &Instant, rtt_millis: &f32) {
        self.next_send_actions = self.world_channel.take_next_actions(now, rtt_millis);
    }

    fn collect_dropped_update_packets(&mut self, rtt_millis: &f32) {
        let drop_duration = Duration::from_millis((DROP_UPDATE_RTT_FACTOR * rtt_millis) as u64);

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
            for (component_index, diff_mask) in &diff_mask_map {
                let (entity, component) = component_index;
                if !self
                    .world_channel
                    .diff_handler
                    .has_component(entity, component)
                {
                    continue;
                }
                let mut new_diff_mask = diff_mask.clone();

                // walk from dropped packet up to most recently sent packet
                if dropped_packet_index == self.last_update_packet_index {
                    continue;
                }

                let mut packet_index = dropped_packet_index.wrapping_add(1);
                while packet_index != self.last_update_packet_index {
                    if let Some((_, diff_mask_map)) = self.sent_updates.get(&packet_index) {
                        if let Some(next_diff_mask) = diff_mask_map.get(component_index) {
                            new_diff_mask.nand(next_diff_mask);
                        }
                    }

                    packet_index = packet_index.wrapping_add(1);
                }

                self.world_channel
                    .diff_handler
                    .or_diff_mask(entity, component, &new_diff_mask);
            }
        }
    }

    fn collect_component_updates(&mut self) {
        self.next_send_updates = self.world_channel.collect_next_updates();
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

        if !self
            .sent_action_packets
            .contains_scan_from_back(packet_index)
        {
            self.sent_action_packets
                .insert_scan_from_back(*packet_index, (now.clone(), Vec::new()));
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

    #[allow(clippy::too_many_arguments)]
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
        let (action_id, action) = self.next_send_actions.get(action_index).unwrap();

        // write message id
        Self::write_action_id(bit_writer, last_written_id, action_id);

        match action {
            EntityActionEvent::SpawnEntity(entity) => {
                EntityActionType::SpawnEntity.ser(bit_writer);

                // write net entity
                self.world_channel
                    .entity_to_net_entity(entity)
                    .unwrap()
                    .ser(bit_writer);

                // get component list
                let component_kinds = match world_record.component_kinds(entity) {
                    Some(kind_list) => kind_list,
                    None => Vec::new(),
                };

                // write number of components
                let components_num =
                    UnsignedVariableInteger::<3>::new(component_kinds.len() as i128);
                components_num.ser(bit_writer);

                for component_kind in &component_kinds {
                    let converter = EntityConverter::new(world_record, self);

                    // write component payload
                    world
                        .component_of_kind(entity, component_kind)
                        .expect("Component does not exist in World")
                        .write(bit_writer, &converter);
                }

                // if we are writing to this packet, add it to record
                if is_writing {
                    //info!("write SpawnEntity({})", action_id);

                    Self::record_action_written(
                        &mut self.sent_action_packets,
                        packet_index,
                        action_id,
                        EntityAction::SpawnEntity(*entity, component_kinds),
                    );
                }
            }
            EntityActionEvent::DespawnEntity(entity) => {
                EntityActionType::DespawnEntity.ser(bit_writer);

                // write net entity
                self.world_channel
                    .entity_to_net_entity(entity)
                    .unwrap()
                    .ser(bit_writer);

                // if we are writing to this packet, add it to record
                if is_writing {
                    //info!("write DespawnEntity({})", action_id);

                    Self::record_action_written(
                        &mut self.sent_action_packets,
                        packet_index,
                        action_id,
                        EntityAction::DespawnEntity(*entity),
                    );
                }
            }
            EntityActionEvent::InsertComponent(entity, component) => {
                if !world.has_component_of_kind(entity, component)
                    || !self.world_channel.entity_channel_is_open(entity)
                {
                    EntityActionType::Noop.ser(bit_writer);

                    // if we are actually writing this packet
                    if is_writing {
                        //info!("write Noop({})", action_id);

                        // add it to action record
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::Noop,
                        );
                    }
                } else {
                    EntityActionType::InsertComponent.ser(bit_writer);

                    // write net entity
                    self.world_channel
                        .entity_to_net_entity(entity)
                        .unwrap()
                        .ser(bit_writer);

                    let converter = EntityConverter::new(world_record, self);

                    // write component payload
                    world
                        .component_of_kind(entity, component)
                        .expect("Component does not exist in World")
                        .write(bit_writer, &converter);

                    // if we are actually writing this packet
                    if is_writing {
                        //info!("write InsertComponent({})", action_id);

                        // add it to action record
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::InsertComponent(*entity, *component),
                        );
                    }
                }
            }
            EntityActionEvent::RemoveComponent(entity, component) => {
                if !self.world_channel.entity_channel_is_open(entity) {
                    EntityActionType::Noop.ser(bit_writer);

                    // if we are actually writing this packet
                    if is_writing {
                        //info!("write Noop({})", action_id);

                        // add it to action record
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::Noop,
                        );
                    }
                } else {
                    EntityActionType::RemoveComponent.ser(bit_writer);

                    // write net entity
                    self.world_channel
                        .entity_to_net_entity(entity)
                        .unwrap()
                        .ser(bit_writer);

                    // write component kind
                    component.ser(bit_writer);

                    // if we are writing to this packet, add it to record
                    if is_writing {
                        //info!("write RemoveComponent({})", action_id);

                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::RemoveComponent(*entity, *component),
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn record_action_written(
        sent_actions: &mut SequenceList<(Instant, Vec<(ActionId, EntityAction<E, P::Kind>)>)>,
        packet_index: &PacketIndex,
        action_id: &ActionId,
        action_record: EntityAction<E, P::Kind>,
    ) {
        let (_, sent_actions_list) = sent_actions.get_mut_scan_from_back(packet_index).unwrap();
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
            let all_update_entities: Vec<E> = self.next_send_updates.keys().copied().collect();

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

        if !self.sent_updates.contains_key(packet_index) {
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
        entity: &E,
        is_writing: bool,
    ) {
        let mut update_holder: Option<HashSet<P::Kind>> = None;
        if is_writing {
            update_holder = Some(
                self.next_send_updates
                    .remove(entity)
                    .expect("should be an update available to pop"),
            );
        }
        let component_set = {
            if is_writing {
                update_holder.as_ref().unwrap()
            } else {
                self.next_send_updates.get(entity).as_ref().unwrap()
            }
        };

        // write net entity
        self.world_channel
            .entity_to_net_entity(entity)
            .unwrap()
            .ser(bit_writer);

        // write number of components
        UnsignedVariableInteger::<3>::new(component_set.len() as u64).ser(bit_writer);

        for component_kind in component_set {
            // write component kind
            component_kind.ser(bit_writer);

            // get diff mask
            let diff_mask = self
                .world_channel
                .diff_handler
                .diff_mask(entity, component_kind)
                .expect("DiffHandler does not have registered Component!")
                .clone();

            // write payload
            {
                let converter = EntityConverter::new(world_record, self);
                world
                    .component_of_kind(entity, component_kind)
                    .expect("Component does not exist in World")
                    .write_update(&diff_mask, bit_writer, &converter);
            }

            ////////
            if is_writing {
                //info!("writing UpdateComponent");

                // place diff mask in a special transmission record - like map
                self.last_update_packet_index = *packet_index;

                let (_, sent_updates_map) = self.sent_updates.get_mut(packet_index).unwrap();
                sent_updates_map.insert((*entity, *component_kind), diff_mask);

                // having copied the diff mask for this update, clear the component
                self.world_channel
                    .diff_handler
                    .clear_diff_mask(entity, component_kind);
            }
        }
    }
}

// PacketNotifiable
impl<P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex> PacketNotifiable
    for EntityManager<P, E, C>
{
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        // Updates
        self.sent_updates.remove(&packet_index);

        // Actions
        if let Some((_, action_list)) = self
            .sent_action_packets
            .remove_scan_from_front(&packet_index)
        {
            for (action_id, action) in action_list {
                self.world_channel.action_delivered(action_id, action);
            }
        }
    }
}

// NetEntityConverter
impl<P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex> NetEntityConverter<E>
    for EntityManager<P, E, C>
{
    fn entity_to_net_entity(&self, entity: &E) -> NetEntity {
        return *self
            .world_channel
            .entity_to_net_entity(entity)
            .expect("entity does not exist for this connection!");
    }

    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> E {
        return *self
            .world_channel
            .net_entity_to_entity(net_entity)
            .expect("entity does not exist for this connection!");
    }
}
