use std::collections::{hash_map::Keys, HashMap, HashSet, VecDeque};

use log::warn;

use naia_shared::{
    DiffMask, LocalComponentKey, LocalEntityKey, LocalObjectKey, LocalReplicaKey, Manifest,
    NaiaKey, PacketReader, PawnKey, ProtocolType, ReplicaActionType,
};

use super::{dual_command_receiver::DualCommandReceiver, replica_action::ReplicaAction};

#[derive(Debug)]
pub struct ReplicaManager<T: ProtocolType> {
    local_replica_store: HashMap<LocalReplicaKey, T>,
    queued_incoming_messages: VecDeque<ReplicaAction<T>>,
    pawn_store: HashMap<LocalObjectKey, T>,
    local_entity_store: HashMap<LocalEntityKey, HashSet<LocalComponentKey>>,
    pawn_entity_store: HashSet<LocalEntityKey>,
    component_entity_map: HashMap<LocalComponentKey, LocalEntityKey>,
}

impl<T: ProtocolType> ReplicaManager<T> {
    pub fn new() -> Self {
        ReplicaManager {
            queued_incoming_messages: VecDeque::new(),
            local_replica_store: HashMap::new(),
            pawn_store: HashMap::new(),
            local_entity_store: HashMap::new(),
            pawn_entity_store: HashSet::new(),
            component_entity_map: HashMap::new(),
        }
    }

    pub fn process_data(
        &mut self,
        manifest: &Manifest<T>,
        command_receiver: &mut DualCommandReceiver<T>,
        packet_tick: u16,
        packet_index: u16,
        reader: &mut PacketReader,
    ) {
        let replica_action_count = reader.read_u8();

        for _ in 0..replica_action_count {
            let message_type = ReplicaActionType::from_u8(reader.read_u8());

            match message_type {
                ReplicaActionType::CreateObject => {
                    // Object Creation
                    let naia_id: u16 = reader.read_u16();
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    let new_replica = manifest.create_replica(naia_id, reader);
                    if !self.local_replica_store.contains_key(&object_key) {
                        self.local_replica_store.insert(object_key, new_replica);

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::CreateObject(object_key));
                    } else {
                        // may have received a duplicate message
                        warn!("attempted to insert duplicate local object key");
                    }
                }
                ReplicaActionType::DeleteReplica => {
                    // Replica Deletion
                    let replica_key = LocalReplicaKey::from_u16(reader.read_u16());

                    if let Some(entity_key) = self.component_entity_map.remove(&replica_key) {
                        // Replica is a Component
                        let component_set = self
                            .local_entity_store
                            .get_mut(&entity_key)
                            .expect("entity not instantiated properly?");
                        if !component_set.remove(&replica_key) {
                            panic!("trying to delete non-existent component");
                        }
                        self.component_delete_cleanup(&entity_key, &replica_key);
                    } else {
                        // Replica is an Object
                        self.object_delete_cleanup(command_receiver, &replica_key);
                    }
                }
                ReplicaActionType::UpdateReplica => {
                    // Replica Update
                    let replica_key = LocalReplicaKey::from_u16(reader.read_u16());

                    if let Some(replica_ref) = self.local_replica_store.get_mut(&replica_key) {
                        let diff_mask: DiffMask = DiffMask::read(reader);

                        replica_ref.read_partial(&diff_mask, reader, packet_index);

                        if let Some(entity_key) = self.component_entity_map.get(&replica_key) {
                            // Replica is a Component

                            // if Entity is a Pawn, replay commands
                            if self.pawn_entity_store.contains(entity_key) {
                                let pawn_key = PawnKey::Entity(*entity_key);
                                command_receiver.replay_commands(packet_tick, &pawn_key);

                                // remove command history until the tick that has already been
                                // checked
                                command_receiver.remove_history_until(packet_tick, &pawn_key);
                            }

                            self.queued_incoming_messages.push_back(
                                ReplicaAction::UpdateComponent(*entity_key, replica_key),
                            );
                        } else {
                            // Replica is an Object
                            self.queued_incoming_messages
                                .push_back(ReplicaAction::UpdateObject(replica_key));
                        }
                    }
                }
                ReplicaActionType::AssignPawn => {
                    // Assign Pawn
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if let Some(protocol) = self.local_replica_store.get(&object_key) {
                        self.pawn_store.insert(object_key, protocol.copy());

                        let pawn_key = PawnKey::Object(object_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::AssignPawn(object_key));
                    }
                }
                ReplicaActionType::UnassignPawn => {
                    // Unassign Pawn
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());
                    if self.pawn_store.contains_key(&object_key) {
                        self.pawn_store.remove(&object_key);

                        let pawn_key = PawnKey::Object(object_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ReplicaAction::UnassignPawn(object_key));
                }
                ReplicaActionType::UpdatePawn => {
                    // Pawn Update
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if let Some(object_ref) = self.local_replica_store.get_mut(&object_key) {
                        object_ref.read_full(reader, packet_index);

                        let pawn_key = PawnKey::Object(object_key);

                        command_receiver.replay_commands(packet_tick, &pawn_key);

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, &pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::UpdateObject(object_key));
                    }
                }
                ReplicaActionType::CreateEntity => {
                    // Entity Creation
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let components_num = reader.read_u8();
                    if self.local_entity_store.contains_key(&entity_key) {
                        // its possible we received a very late duplicate message
                        warn!("attempted to insert duplicate entity");
                        // continue reading, just don't do anything with the data
                        for _ in 0..components_num {
                            let naia_id: u16 = reader.read_u16();
                            let _component_key = reader.read_u16();
                            manifest.create_replica(naia_id, reader);
                        }
                    } else {
                        let mut component_list: Vec<LocalComponentKey> = Vec::new();
                        let mut component_set = HashSet::new();

                        for _ in 0..components_num {
                            // Component Creation
                            let naia_id: u16 = reader.read_u16();
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_replica = manifest.create_replica(naia_id, reader);
                            if self.local_replica_store.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                self.local_replica_store.insert(component_key, new_replica);
                                self.component_entity_map.insert(component_key, entity_key);
                                component_list.push(component_key);
                                component_set.insert(component_key);
                            }
                        }

                        self.local_entity_store.insert(entity_key, component_set);

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::CreateEntity(entity_key, component_list));
                    }
                }
                ReplicaActionType::DeleteEntity => {
                    // Entity Deletion
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());

                    if let Some(component_set) = self.local_entity_store.remove(&entity_key) {
                        if self.pawn_entity_store.take(&entity_key).is_some() {
                            let pawn_key = PawnKey::Entity(entity_key);
                            command_receiver.pawn_cleanup(&pawn_key);
                        }

                        for component_key in component_set {
                            // delete all components
                            self.component_delete_cleanup(&entity_key, &component_key);

                            self.component_entity_map.remove(&component_key);
                        }

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::DeleteEntity(entity_key));
                    } else {
                        // its possible we received a very late duplicate message
                        warn!(
                            "received message attempting to delete nonexistent entity: {}",
                            entity_key.to_u16()
                        );
                    }
                }
                ReplicaActionType::AssignPawnEntity => {
                    // Assign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if let Some(component_set) = self.local_entity_store.get(&entity_key) {
                        self.pawn_entity_store.insert(entity_key);

                        // create copies of components
                        for component_key in component_set {
                            if let Some(protocol) = self.local_replica_store.get(&component_key) {
                                self.pawn_store.insert(*component_key, protocol.copy());
                            }
                        }
                        //

                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicaAction::AssignPawnEntity(entity_key));
                    }
                }
                ReplicaActionType::UnassignPawnEntity => {
                    // Unassign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.pawn_entity_store.contains(&entity_key) {
                        self.pawn_entity_store.remove(&entity_key);

                        // remove pawn components
                        let component_set = self.local_entity_store.get(&entity_key).unwrap();
                        for component_key in component_set {
                            self.pawn_store.remove(&component_key);
                        }
                        //

                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ReplicaAction::UnassignPawnEntity(entity_key));
                }
                ReplicaActionType::AddComponent => {
                    //TODO: handle adding Component to a Pawn...

                    // Add Component to Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let naia_id: u16 = reader.read_u16();
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    let new_component = manifest.create_replica(naia_id, reader);
                    if self.local_replica_store.contains_key(&component_key) {
                        // its possible we received a very late duplicate message
                        warn!(
                            "attempting to add duplicate local component key: {}, into entity: {}",
                            component_key.to_u16(),
                            entity_key.to_u16()
                        );
                    } else {
                        if !self.local_entity_store.contains_key(&entity_key) {
                            // its possible we received a very late duplicate message
                            warn!(
                                "attempting to add a component: {}, to nonexistent entity: {}",
                                component_key.to_u16(),
                                entity_key.to_u16()
                            );
                        } else {
                            self.local_replica_store
                                .insert(component_key, new_component);

                            self.component_entity_map.insert(component_key, entity_key);
                            let component_set =
                                self.local_entity_store.get_mut(&entity_key).unwrap();

                            component_set.insert(component_key);

                            self.queued_incoming_messages
                                .push_back(ReplicaAction::AddComponent(entity_key, component_key));
                        }
                    }
                }
                ReplicaActionType::Unknown => {
                    panic!("received unknown type of replica action");
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ReplicaAction<T>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn object_keys(&self) -> Vec<LocalObjectKey> {
        let mut output: Vec<LocalObjectKey> = Vec::new();
        for key in self.local_replica_store.keys() {
            if !self.component_entity_map.contains_key(key) {
                output.push(*key);
            }
        }
        output
    }

    pub fn component_keys(&self) -> Vec<LocalComponentKey> {
        let mut output: Vec<LocalComponentKey> = Vec::new();
        for key in self.component_entity_map.keys() {
            output.push(*key);
        }
        output
    }

    pub fn get_object(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.local_replica_store.get(key);
    }

    pub fn has_object(&self, key: &LocalObjectKey) -> bool {
        return self.local_replica_store.contains_key(key);
    }

    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn get_components(&self, key: &LocalEntityKey) -> Vec<T> {
        let mut output = Vec::new();
        if let Some(component_set) = self.local_entity_store.get(key) {
            for component_key in component_set {
                if let Some(component_proto) = self.local_replica_store.get(component_key) {
                    output.push(component_proto.clone());
                }
            }
        }
        return output;
    }

    pub fn get_pawn_components(&self, key: &LocalEntityKey) -> Vec<T> {
        let mut output = Vec::new();
        if self.pawn_entity_store.contains(key) {
            if let Some(component_set) = self.local_entity_store.get(key) {
                for component_key in component_set {
                    if let Some(component_proto) = self.pawn_store.get(component_key) {
                        output.push(component_proto.clone());
                    }
                }
            }
        }
        return output;
    }

    pub fn pawn_keys(&self) -> Keys<LocalObjectKey, T> {
        return self.pawn_store.keys();
    }

    pub fn get_pawn(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.pawn_store.get(key);
    }

    pub fn pawn_reset(&mut self, key: &LocalObjectKey) {
        if let Some(object_ref) = self.local_replica_store.get(key) {
            if let Some(pawn_ref) = self.pawn_store.get_mut(key) {
                pawn_ref.mirror(object_ref);
            }
        }
        self.queued_incoming_messages
            .push_back(ReplicaAction::ResetPawn(*key));
    }

    pub fn pawn_reset_entity(&mut self, key: &LocalEntityKey) {
        if let Some(component_set) = self.local_entity_store.get(key) {
            for component_key in component_set {
                if let Some(component_ref) = self.local_replica_store.get(component_key) {
                    if let Some(pawn_component_ref) = self.pawn_store.get_mut(component_key) {
                        pawn_component_ref.mirror(component_ref);
                    }
                }
            }
        }

        self.queued_incoming_messages
            .push_back(ReplicaAction::ResetPawnEntity(*key));
    }

    // internal

    fn object_delete_cleanup(
        &mut self,
        command_receiver: &mut DualCommandReceiver<T>,
        object_key: &LocalObjectKey,
    ) {
        if let Some(object) = self.local_replica_store.remove(&object_key) {
            if self.pawn_store.contains_key(&object_key) {
                self.pawn_store.remove(&object_key);
                let pawn_key = PawnKey::Object(*object_key);
                command_receiver.pawn_cleanup(&pawn_key);
            }

            self.queued_incoming_messages
                .push_back(ReplicaAction::DeleteObject(*object_key, object));
        }
    }

    fn component_delete_cleanup(
        &mut self,
        entity_key: &LocalEntityKey,
        component_key: &LocalComponentKey,
    ) {
        if let Some(component) = self.local_replica_store.remove(&component_key) {
            self.queued_incoming_messages
                .push_back(ReplicaAction::RemoveComponent(
                    *entity_key,
                    *component_key,
                    component,
                ));
        }
    }
}
