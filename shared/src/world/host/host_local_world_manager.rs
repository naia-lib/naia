use std::{
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{messages::channels::senders::indexed_message_writer::IndexedMessageWriter, BitWrite, BitWriter, ChannelKind, ComponentKind, ComponentKinds, ConstBitLength, DiffMask, EntityAction, EntityActionType, EntityConverter, Instant, MessageContainer, MessageIndex, MessageKinds, MessageManager, NetEntity, NetEntityConverter, PacketIndex, PacketNotifiable, Serde, UnsignedVariableInteger, WorldRefType, EntityDoesNotExistError};

use super::{
    entity_action_event::EntityActionEvent, global_diff_handler::GlobalDiffHandler,
    sequence_list::SequenceList, world_channel::WorldChannel, world_record::WorldRecord,
};

const DROP_UPDATE_RTT_FACTOR: f32 = 1.5;
const ACTION_RECORD_TTL: Duration = Duration::from_secs(60);

pub type ActionId = MessageIndex;

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct HostLocalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    // World
    world_channel: WorldChannel<E>,

    // Actions
    next_send_actions: VecDeque<(ActionId, EntityActionEvent<E>)>,
    sent_action_packets: SequenceList<(Instant, Vec<(ActionId, EntityAction<E>)>)>,

    // Updates
    next_send_updates: HashMap<E, HashSet<ComponentKind>>,
    /// Map of component updates and [`DiffMask`] that were written into each packet
    sent_updates: HashMap<PacketIndex, (Instant, HashMap<(E, ComponentKind), DiffMask>)>,
    /// Last [`PacketIndex`] where a component update was written by the server
    last_update_packet_index: PacketIndex,
}

impl<E: Copy + Eq + Hash + Send + Sync> HostLocalWorldManager<E> {
    /// Create a new HostWorldManager, given the client's address
    pub fn new(address: SocketAddr, diff_handler: &Arc<RwLock<GlobalDiffHandler<E>>>) -> Self {
        HostLocalWorldManager {
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

    // World

    // used for
    pub fn init_entity(&mut self, entity: &E, component_kinds: Vec<ComponentKind>) {
        // add entity
        self.spawn_entity(entity);
        // add components
        for component_kind in component_kinds {
            self.insert_component(entity, &component_kind);
        }
    }

    pub fn spawn_entity(&mut self, entity: &E) {
        self.world_channel.host_spawn_entity(entity);
    }

    pub fn despawn_entity(&mut self, entity: &E) {
        self.world_channel.host_despawn_entity(entity);
    }

    pub fn insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.world_channel
            .host_insert_component(entity, component_kind);
    }

    pub fn remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.world_channel
            .host_remove_component(entity, component_kind);
    }

    pub fn host_has_entity(&self, entity: &E) -> bool {
        self.world_channel.host_has_entity(entity)
    }

    pub fn entity_channel_is_open(&self, entity: &E) -> bool {
        self.world_channel.entity_channel_is_open(entity)
    }

    // Messages

    pub fn queue_entity_message(
        &mut self,
        entities: Vec<E>,
        channel: &ChannelKind,
        message: MessageContainer,
    ) {
        self.world_channel
            .delayed_entity_messages
            .queue_message(entities, channel, message);
    }

    // Writer

    pub fn collect_outgoing_messages(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
        world_record: &WorldRecord<E>,
        message_kinds: &MessageKinds,
        message_manager: &mut MessageManager,
    ) {
        let messages = self
            .world_channel
            .delayed_entity_messages
            .collect_ready_messages();
        let converter = EntityConverter::new(world_record, self);
        for (channel_kind, message) in messages {
            message_manager.send_message(message_kinds, &converter, &channel_kind, message);
        }

        self.collect_dropped_update_packets(rtt_millis);

        self.collect_dropped_action_packets();
        self.collect_next_actions(now, rtt_millis);

        self.collect_component_updates();
    }

    pub fn has_outgoing_messages(&self) -> bool {
        !self.next_send_actions.is_empty() || !self.next_send_updates.is_empty()
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
        writer: &mut dyn BitWrite,
        last_id_opt: &mut Option<ActionId>,
        current_id: &ActionId,
    ) {
        IndexedMessageWriter::write_message_index(writer, last_id_opt, current_id);
        *last_id_opt = Some(*current_id);
    }

    pub fn write_actions<W: WorldRefType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E>,
        has_written: &mut bool,
    ) {
        let mut last_counted_id: Option<MessageIndex> = None;
        let mut last_written_id: Option<MessageIndex> = None;

        loop {
            if self.next_send_actions.is_empty() {
                break;
            }

            // check that we can write the next message
            let mut counter = writer.counter();
            self.write_action(
                component_kinds,
                world,
                world_record,
                packet_index,
                &mut counter,
                &mut last_counted_id,
                false,
            );

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of component being too big
                if !*has_written {
                    self.warn_overflow_action(
                        component_kinds,
                        world_record,
                        counter.bits_needed(),
                        writer.bits_free(),
                    );
                }
                break;
            }

            *has_written = true;

            // write ActionContinue bit
            true.ser(writer);

            // optimization
            if !self
                .sent_action_packets
                .contains_scan_from_back(packet_index)
            {
                self.sent_action_packets
                    .insert_scan_from_back(*packet_index, (now.clone(), Vec::new()));
            }

            // write data
            self.write_action(
                component_kinds,
                world,
                world_record,
                packet_index,
                writer,
                &mut last_written_id,
                true,
            );

            // pop action we've written
            self.next_send_actions.pop_front();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_action<W: WorldRefType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        world: &W,
        world_record: &WorldRecord<E>,
        packet_index: &PacketIndex,
        writer: &mut dyn BitWrite,
        last_written_id: &mut Option<ActionId>,
        is_writing: bool,
    ) {
        let (action_id, action) = self.next_send_actions.front().unwrap();

        // write message id
        Self::write_action_id(writer, last_written_id, action_id);

        match action {
            EntityActionEvent::SpawnEntity(entity) => {
                EntityActionType::SpawnEntity.ser(writer);

                // write net entity
                self.world_channel
                    .entity_to_net_entity(entity)
                    .unwrap()
                    .ser(writer);

                // get component list
                let component_kind_list = match world_record.component_kinds(entity) {
                    Some(kind_list) => kind_list,
                    None => Vec::new(),
                };

                // write number of components
                let components_num =
                    UnsignedVariableInteger::<3>::new(component_kind_list.len() as i128);
                components_num.ser(writer);

                for component_kind in &component_kind_list {
                    let converter = EntityConverter::new(world_record, self);

                    // write component payload
                    world
                        .component_of_kind(entity, component_kind)
                        .expect("Component does not exist in World")
                        .write(component_kinds, writer, &converter);
                }

                // if we are writing to this packet, add it to record
                if is_writing {
                    Self::record_action_written(
                        &mut self.sent_action_packets,
                        packet_index,
                        action_id,
                        EntityAction::SpawnEntity(*entity, component_kind_list),
                    );
                }
            }
            EntityActionEvent::DespawnEntity(entity) => {
                EntityActionType::DespawnEntity.ser(writer);

                // write net entity
                self.world_channel
                    .entity_to_net_entity(entity)
                    .unwrap()
                    .ser(writer);

                // if we are writing to this packet, add it to record
                if is_writing {
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
                    EntityActionType::Noop.ser(writer);

                    // if we are actually writing this packet
                    if is_writing {
                        // add it to action record
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::Noop,
                        );
                    }
                } else {
                    EntityActionType::InsertComponent.ser(writer);

                    // write net entity
                    self.world_channel
                        .entity_to_net_entity(entity)
                        .unwrap()
                        .ser(writer);

                    let converter = EntityConverter::new(world_record, self);

                    // write component payload
                    world
                        .component_of_kind(entity, component)
                        .expect("Component does not exist in World")
                        .write(component_kinds, writer, &converter);

                    // if we are actually writing this packet
                    if is_writing {
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
            EntityActionEvent::RemoveComponent(entity, component_kind) => {
                if !self.world_channel.entity_channel_is_open(entity) {
                    EntityActionType::Noop.ser(writer);

                    // if we are actually writing this packet
                    if is_writing {
                        // add it to action record
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::Noop,
                        );
                    }
                } else {
                    EntityActionType::RemoveComponent.ser(writer);

                    // write net entity
                    self.world_channel
                        .entity_to_net_entity(entity)
                        .unwrap()
                        .ser(writer);

                    // write component kind
                    component_kind.ser(component_kinds, writer);

                    // if we are writing to this packet, add it to record
                    if is_writing {
                        Self::record_action_written(
                            &mut self.sent_action_packets,
                            packet_index,
                            action_id,
                            EntityAction::RemoveComponent(*entity, *component_kind),
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn record_action_written(
        sent_actions: &mut SequenceList<(Instant, Vec<(ActionId, EntityAction<E>)>)>,
        packet_index: &PacketIndex,
        action_id: &ActionId,
        action_record: EntityAction<E>,
    ) {
        let (_, sent_actions_list) = sent_actions.get_mut_scan_from_back(packet_index).unwrap();
        sent_actions_list.push((*action_id, action_record));
    }

    fn warn_overflow_action(
        &self,
        component_kinds: &ComponentKinds,
        world_record: &WorldRecord<E>,
        bits_needed: u32,
        bits_free: u32,
    ) {
        let (_action_id, action) = self.next_send_actions.front().unwrap();

        match action {
            EntityActionEvent::SpawnEntity(entity) => {
                let component_kind_list = match world_record.component_kinds(entity) {
                    Some(kind_list) => kind_list,
                    None => Vec::new(),
                };

                let mut component_names = "".to_owned();
                let mut added = false;

                for component_kind in &component_kind_list {
                    if added {
                        component_names.push(',');
                    } else {
                        added = true;
                    }
                    let name = component_kinds.kind_to_name(component_kind);
                    component_names.push_str(&name);
                }
                panic!(
                    "Packet Write Error: Blocking overflow detected! Entity Spawn message with Components `{component_names}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommend slimming down these Components."
                )
            }
            EntityActionEvent::InsertComponent(_entity, component_kind) => {
                let component_name = component_kinds.kind_to_name(component_kind);
                panic!(
                    "Packet Write Error: Blocking overflow detected! Component Insertion message of type `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
                )
            }
            _ => {
                panic!(
                    "Packet Write Error: Blocking overflow detected! Action requires {bits_needed} bits, but packet only has {bits_free} bits available! This message should never display..."
                )
            }
        }
    }

    pub fn write_updates<W: WorldRefType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: &PacketIndex,
        world: &W,
        world_record: &WorldRecord<E>,
        has_written: &mut bool,
    ) {
        let all_update_entities: Vec<E> = self.next_send_updates.keys().copied().collect();

        for entity in all_update_entities {
            // check that we can at least write a NetEntityId and a ComponentContinue bit
            let mut counter = writer.counter();

            counter.write_bits(<NetEntity as ConstBitLength>::const_bit_length());
            counter.write_bit(false);

            if counter.overflowed() {
                break;
            }

            // write UpdateContinue bit
            true.ser(writer);

            // reserve ComponentContinue bit
            writer.reserve_bits(1);

            // write NetEntityId
            let net_entity_id = self.world_channel.entity_to_net_entity(&entity).unwrap();
            net_entity_id.ser(writer);

            // write Components
            self.write_update(
                component_kinds,
                now,
                world,
                world_record,
                packet_index,
                writer,
                &entity,
                has_written,
            );

            // write ComponentContinue finish bit, release
            false.ser(writer);
            writer.release_bits(1);
        }
    }

    /// For a given entity, write component value updates into a packet
    /// Only component values that changed in the internal (naia's) host world will be written
    fn write_update<W: WorldRefType<E>>(
        &mut self,
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        world_record: &WorldRecord<E>,
        packet_index: &PacketIndex,
        writer: &mut BitWriter,
        entity: &E,
        has_written: &mut bool,
    ) {
        let mut written_component_kinds = Vec::new();
        let component_kind_set = self.next_send_updates.get(entity).unwrap();
        for component_kind in component_kind_set {
            // get diff mask
            let diff_mask = self
                .world_channel
                .diff_handler
                .diff_mask(entity, component_kind)
                .expect("DiffHandler does not have registered Component!")
                .clone();

            let converter = EntityConverter::new(world_record, self);

            // check that we can write the next component update
            let mut counter = writer.counter();
            counter.write_bits(<ComponentKind as ConstBitLength>::const_bit_length());
            world
                .component_of_kind(entity, component_kind)
                .expect("Component does not exist in World")
                .write_update(&diff_mask, &mut counter, &converter);

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of component being too big
                if !*has_written {
                    let component_name = component_kinds.kind_to_name(component_kind);
                    self.warn_overflow_update(
                        component_name,
                        counter.bits_needed(),
                        writer.bits_free(),
                    );
                }

                break;
            }

            *has_written = true;

            // write ComponentContinue bit
            true.ser(writer);

            // write component kind
            component_kind.ser(component_kinds, writer);

            // write data
            world
                .component_of_kind(entity, component_kind)
                .expect("Component does not exist in World")
                .write_update(&diff_mask, writer, &converter);

            written_component_kinds.push(*component_kind);

            // place diff mask in a special transmission record - like map
            self.last_update_packet_index = *packet_index;

            if !self.sent_updates.contains_key(packet_index) {
                self.sent_updates
                    .insert(*packet_index, (now.clone(), HashMap::new()));
            }
            let (_, sent_updates_map) = self.sent_updates.get_mut(packet_index).unwrap();
            sent_updates_map.insert((*entity, *component_kind), diff_mask);

            // having copied the diff mask for this update, clear the component
            self.world_channel
                .diff_handler
                .clear_diff_mask(entity, component_kind);
        }

        let update_kinds = self.next_send_updates.get_mut(entity).unwrap();
        for component_kind in &written_component_kinds {
            update_kinds.remove(component_kind);
        }
        if update_kinds.is_empty() {
            self.next_send_updates.remove(entity);
        }
    }

    fn warn_overflow_update(&self, component_name: String, bits_needed: u32, bits_free: u32) {
        panic!(
            "Packet Write Error: Blocking overflow detected! Data update of Component `{component_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Component"
        )
    }
}

// PacketNotifiable
impl<E: Copy + Eq + Hash + Send + Sync> PacketNotifiable for HostLocalWorldManager<E> {
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
impl<E: Copy + Eq + Hash + Send + Sync> NetEntityConverter<E> for HostLocalWorldManager<E> {
    fn entity_to_net_entity(&self, entity: &E) -> Result<NetEntity, EntityDoesNotExistError> {
        if let Some(net_entity) = self
            .world_channel
            .entity_to_net_entity(entity) {
            return Ok(*net_entity);
        }
        return Err(EntityDoesNotExistError);
    }

    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self
            .world_channel
            .net_entity_to_entity(net_entity) {
            return Ok(*entity);
        }
        return Err(EntityDoesNotExistError);
    }
}
