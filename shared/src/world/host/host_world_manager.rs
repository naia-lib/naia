use std::{
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    time::Duration,
};

use crate::{sequence_list::SequenceList, world::{
    entity::entity_converters::GlobalWorldManagerType, local_world_manager::LocalWorldManager,
}, ComponentKind, DiffMask, EntityAction, EntityAndGlobalEntityConverter, GlobalEntity, HostEntity, Instant, MessageIndex, PacketIndex, WorldRefType};

use super::{entity_action_event::EntityActionEvent, world_channel::WorldChannel};

const DROP_UPDATE_RTT_FACTOR: f32 = 1.5;
const ACTION_RECORD_TTL: Duration = Duration::from_secs(60);

pub type ActionId = MessageIndex;

/// Manages Entities for a given Client connection and keeps them in
/// sync on the Client
pub struct HostWorldManager {
    // World
    pub world_channel: WorldChannel,

    // Actions
    pub sent_action_packets: SequenceList<(Instant, Vec<(ActionId, EntityAction<GlobalEntity>)>)>,

    // Updates
    /// Map of component updates and [`DiffMask`] that were written into each packet
    pub sent_updates: HashMap<PacketIndex, (Instant, HashMap<(GlobalEntity, ComponentKind), DiffMask>)>,
    /// Last [`PacketIndex`] where a component update was written by the server
    pub last_update_packet_index: PacketIndex,
}

pub struct HostWorldEvents {
    pub next_send_actions: VecDeque<(ActionId, EntityActionEvent)>,
    pub next_send_updates: HashMap<GlobalEntity, HashSet<ComponentKind>>,
}

impl HostWorldEvents {
    pub fn has_events(&self) -> bool {
        !self.next_send_actions.is_empty() || !self.next_send_updates.is_empty()
    }
}

impl HostWorldManager {
    /// Create a new HostWorldManager, given the client's address
    pub fn new(
        address: &Option<SocketAddr>,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> Self {
        HostWorldManager {
            // World
            world_channel: WorldChannel::new(address, global_world_manager),
            sent_action_packets: SequenceList::new(),

            // Update
            sent_updates: HashMap::new(),
            last_update_packet_index: 0,
        }
    }

    // World

    // used when Entity first comes into Connection's scope
    pub fn init_entity(
        &mut self,
        world_manager: &mut LocalWorldManager,
        entity: &GlobalEntity,
        component_kinds: Vec<ComponentKind>,
    ) {
        // add entity
        self.spawn_entity(world_manager, entity, &component_kinds);
        // add components
        for component_kind in component_kinds {
            self.insert_component(entity, &component_kind);
        }
    }

    pub fn spawn_entity(
        &mut self,
        world_manager: &mut LocalWorldManager,
        entity: &GlobalEntity,
        component_kinds: &Vec<ComponentKind>,
    ) {
        self.world_channel
            .host_spawn_entity(world_manager, entity, component_kinds);
    }

    pub fn despawn_entity(&mut self, entity: &GlobalEntity) {
        self.world_channel.host_despawn_entity(entity);
    }

    pub fn client_initiated_despawn(&mut self, entity: &GlobalEntity) {
        self.world_channel.client_initiated_despawn(entity);
    }

    pub fn insert_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.world_channel
            .host_insert_component(entity, component_kind);
    }

    pub fn remove_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.world_channel
            .host_remove_component(entity, component_kind);
    }

    pub fn host_has_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.world_channel.host_has_entity(global_entity)
    }

    // used when Remote Entity gains Write Authority (delegation)
    pub fn track_remote_entity(
        &mut self,
        local_world_manager: &mut LocalWorldManager,
        entity: &GlobalEntity,
        component_kinds: Vec<ComponentKind>,
    ) -> HostEntity {
        // add entity
        let new_host_entity =
            self.world_channel
                .track_remote_entity(local_world_manager, entity, &component_kinds);

        // info!("--- tracking remote entity ---");

        // add components
        for component_kind in component_kinds {
            self.track_remote_component(entity, &component_kind);
        }

        // info!("--- ---------------------- ---");

        new_host_entity
    }

    pub fn untrack_remote_entity(
        &mut self,
        local_world_manager: &mut LocalWorldManager,
        entity: &GlobalEntity,
    ) {
        self.world_channel
            .untrack_remote_entity(local_world_manager, entity);
    }

    pub fn track_remote_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.world_channel
            .track_remote_component(entity, component_kind);
    }

    // Messages

    pub fn handle_dropped_packets(&mut self, now: &Instant, rtt_millis: &f32) {
        self.handle_dropped_update_packets(now, rtt_millis);
        self.handle_dropped_action_packets(now);
    }

    // Collecting

    fn handle_dropped_action_packets(&mut self, now: &Instant) {
        let mut pop = false;

        loop {
            if let Some((_, (time_sent, _))) = self.sent_action_packets.front() {
                if time_sent.elapsed(now) > ACTION_RECORD_TTL {
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

    fn handle_dropped_update_packets(&mut self, now: &Instant, rtt_millis: &f32) {
        let drop_duration = Duration::from_millis((DROP_UPDATE_RTT_FACTOR * rtt_millis) as u64);

        {
            let mut dropped_packets = Vec::new();
            for (packet_index, (time_sent, _)) in &self.sent_updates {
                let elapsed_since_send = time_sent.elapsed(now);
                if elapsed_since_send > drop_duration {
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
                if dropped_packet_index != self.last_update_packet_index {
                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                    while packet_index != self.last_update_packet_index {
                        if let Some((_, diff_mask_map)) = self.sent_updates.get(&packet_index) {
                            if let Some(next_diff_mask) = diff_mask_map.get(component_index) {
                                new_diff_mask.nand(next_diff_mask);
                            }
                        }

                        packet_index = packet_index.wrapping_add(1);
                    }
                }

                self.world_channel
                    .diff_handler
                    .or_diff_mask(entity, component, &new_diff_mask);
            }
        }
    }

    pub fn take_outgoing_events<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        now: &Instant,
        rtt_millis: &f32,
    ) -> HostWorldEvents {
        HostWorldEvents {
            next_send_actions: self.world_channel.take_next_actions(now, rtt_millis),
            next_send_updates: self
                .world_channel
                .collect_next_updates(world, converter, global_world_manager),
        }
    }
}

impl HostWorldManager {
    pub fn notify_packet_delivered(
        &mut self,
        packet_index: PacketIndex,
        local_world_manager: &mut LocalWorldManager,
    ) {
        // Updates
        self.sent_updates.remove(&packet_index);

        // Actions
        if let Some((_, action_list)) = self
            .sent_action_packets
            .remove_scan_from_front(&packet_index)
        {
            for (action_id, action) in action_list {
                self.world_channel
                    .action_delivered(local_world_manager, action_id, action);
            }
        }
    }
}
