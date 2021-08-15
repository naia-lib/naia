
use std::collections::{HashSet, HashMap, VecDeque, hash_map::Keys};

use log::warn;

use naia_shared::{ProtocolType, LocalReplicateKey, Manifest, PacketReader, DiffMask,
                  LocalEntityKey, ReplicateActionType, NaiaKey, LocalComponentKey, PawnKey};

use super::{replicate_action::ReplicateAction, dual_command_receiver::DualCommandReceiver};

#[derive(Debug)]
pub struct ReplicateManager<T: ProtocolType> {
    local_replicate_store:              HashMap<LocalReplicateKey, T>,
    queued_incoming_messages:           VecDeque<ReplicateAction<T>>,
    pawn_store:                         HashMap<LocalReplicateKey, T>,
    local_entity_store:                 HashMap<LocalEntityKey, HashSet<LocalComponentKey>>,
    pawn_entity_store:                  HashSet<LocalEntityKey>,
    component_entity_map:               HashMap<LocalComponentKey, LocalEntityKey>,
}

impl<T: ProtocolType> ReplicateManager<T> {
    pub fn new() -> Self {
        ReplicateManager {
            queued_incoming_messages:           VecDeque::new(),
            local_replicate_store:              HashMap::new(),
            pawn_store:                         HashMap::new(),
            local_entity_store:                 HashMap::new(),
            pawn_entity_store:                  HashSet::new(),
            component_entity_map:               HashMap::new(),
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
        let replicate_action_count = reader.read_u8();

        for _ in 0..replicate_action_count {
            let message_type = ReplicateActionType::from_u8(reader.read_u8());

            match message_type {
                ReplicateActionType::CreateObject => {
                    // Replicate Creation
                    let naia_id: u16 = reader.read_u16();
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());

                    let new_replicate = manifest.create_replicate(naia_id, reader);
                    if !self.local_replicate_store.contains_key(&object_key) {
                        self.local_replicate_store.insert(object_key, new_replicate);

                        self.queued_incoming_messages
                            .push_back(ReplicateAction::CreateReplicate(object_key));
                    } else {
                        // may have received a duplicate message
                        warn!("attempted to insert duplicate local replicate key");
                    }
                }
                ReplicateActionType::DeleteObject => {
                    // Replicate Deletion
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());

                    if self.component_entity_map.contains_key(&object_key) {
                        // Replicate is a Component
                        let entity_key = self.component_entity_map.remove(&object_key).unwrap();
                        let component_set = self.local_entity_store.get_mut(&entity_key)
                            .expect("entity not instantiated properly?");
                        if !component_set.remove(&object_key) {
                            panic!("trying to delete non-existent component");
                        }
                        self.component_delete_cleanup(&entity_key, &object_key);
                    } else {
                        // Replicate is a Replicate
                        self.replicate_delete_cleanup(command_receiver, &object_key);
                    }
                }
                ReplicateActionType::UpdateObject => {
                    // Replicate Update
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());

                    if let Some(replicate_ref) = self.local_replicate_store.get_mut(&object_key) {
                        // Replicate is not a Pawn
                        let diff_mask: DiffMask = DiffMask::read(reader);

                        replicate_ref.read_partial(&diff_mask, reader, packet_index);

                        if let Some(entity_key) = self.component_entity_map.get(&object_key) {
                            // Replicate is a Component

                            // if Entity is a Pawn, replay commands
                            if self.pawn_entity_store.contains(entity_key) {
                                let pawn_key = PawnKey::Entity(*entity_key);
                                command_receiver.replay_commands(packet_tick, &pawn_key);

                                // remove command history until the tick that has already been checked
                                command_receiver.remove_history_until(packet_tick, &pawn_key);
                            }

                            self.queued_incoming_messages
                                .push_back(ReplicateAction::UpdateComponent(*entity_key, object_key));
                        } else {
                            // Replicate is an Replicate
                            self.queued_incoming_messages
                                .push_back(ReplicateAction::UpdateReplicate(object_key));
                        }
                    }
                }
                ReplicateActionType::AssignPawn => {
                    // Assign Pawn
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());

                    if let Some(replicate_ref) = self.local_replicate_store.get_mut(&object_key) {
                        self.pawn_store
                            .insert(object_key, replicate_ref.inner_ref().borrow().get_typed_copy());

                        let pawn_key = PawnKey::Replicate(object_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicateAction::AssignPawn(object_key));
                    }
                }
                ReplicateActionType::UnassignPawn => {
                    // Unassign Pawn
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());
                    if self.pawn_store.contains_key(&object_key) {
                        self.pawn_store.remove(&object_key);

                        let pawn_key = PawnKey::Replicate(object_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ReplicateAction::UnassignPawn(object_key));
                }
                ReplicateActionType::UpdatePawn => {
                    // Pawn Update
                    let object_key = LocalReplicateKey::from_u16(reader.read_u16());

                    if let Some(replicate_ref) = self.local_replicate_store.get_mut(&object_key) {
                        replicate_ref.read_full(reader, packet_index);

                        let pawn_key = PawnKey::Replicate(object_key);

                        command_receiver.replay_commands(packet_tick, &pawn_key);

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, &pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicateAction::UpdateReplicate(object_key));
                    }
                }
                ReplicateActionType::CreateEntity => {
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
                            manifest.create_replicate(naia_id, reader);
                        }
                    } else {
                        let mut component_list: Vec<LocalComponentKey> = Vec::new();
                        let mut component_set = HashSet::new();

                        for _ in 0..components_num {
                            // Component Creation
                            let naia_id: u16 = reader.read_u16();
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_replicate = manifest.create_replicate(naia_id, reader);
                            if self.local_replicate_store.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                self.local_replicate_store.insert(component_key, new_replicate);
                                self.component_entity_map.insert(component_key, entity_key);
                                component_list.push(component_key);
                                component_set.insert(component_key);
                            }
                        }

                        self.local_entity_store.insert(entity_key, component_set);

                        self.queued_incoming_messages
                            .push_back(ReplicateAction::CreateEntity(entity_key, component_list));
                    }
                }
                ReplicateActionType::DeleteEntity => {
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
                            .push_back(ReplicateAction::DeleteEntity(entity_key));
                    } else {
                        // its possible we received a very late duplicate message
                        warn!("received message attempting to delete nonexistent entity: {}", entity_key.to_u16());
                    }
                }
                ReplicateActionType::AssignPawnEntity => {
                    // Assign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.local_entity_store.contains_key(&entity_key) {
                        self.pawn_entity_store
                            .insert(entity_key);

                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ReplicateAction::AssignPawnEntity(entity_key));
                    }
                }
                ReplicateActionType::UnassignPawnEntity => {
                    // Unassign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.pawn_entity_store.contains(&entity_key) {
                        self.pawn_entity_store.remove(&entity_key);
                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ReplicateAction::UnassignPawnEntity(entity_key));
                }
                ReplicateActionType::AddComponent => {
                    // Add Component to Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let naia_id: u16 = reader.read_u16();
                    let component_key = LocalReplicateKey::from_u16(reader.read_u16());

                    let new_component = manifest.create_replicate(naia_id, reader);
                    if self.local_replicate_store.contains_key(&component_key) {
                        // its possible we received a very late duplicate message
                        warn!("attempting to add duplicate local component key: {}, into entity: {}",
                               component_key.to_u16(), entity_key.to_u16());
                    } else {
                        if !self.local_entity_store.contains_key(&entity_key) {
                            // its possible we received a very late duplicate message
                            warn!("attempting to add a component: {}, to nonexistent entity: {}",
                                  component_key.to_u16(),
                                  entity_key.to_u16());
                        } else {
                            self.local_replicate_store.insert(component_key, new_component);

                            self.component_entity_map.insert(component_key, entity_key);
                            let component_set = self.local_entity_store.get_mut(&entity_key).unwrap();

                            component_set.insert(component_key);

                            self.queued_incoming_messages
                                .push_back(ReplicateAction::AddComponent(entity_key, component_key));
                        }
                    }
                }
                ReplicateActionType::Unknown => {
                    panic!("received unknown type of replicate message");
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ReplicateAction<T>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn object_keys(&self) -> Vec<LocalReplicateKey> {
        let mut output: Vec<LocalReplicateKey> = Vec::new();
        for key in self.local_replicate_store.keys() {
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

    pub fn get_object(&self, key: &LocalReplicateKey) -> Option<&T> {
        return self.local_replicate_store.get(key);
    }

    pub fn has_object(&self, key: &LocalReplicateKey) -> bool {
        return self.local_replicate_store.contains_key(key);
    }

    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalReplicateKey, T> {
        return self.pawn_store.keys();
    }

    pub fn get_pawn(&self, key: &LocalReplicateKey) -> Option<&T> {
        return self.pawn_store.get(key);
    }

    pub fn pawn_reset(&mut self, key: &LocalReplicateKey) {
        if let Some(replicate_ref) = self.local_replicate_store.get(key) {
            if let Some(pawn_ref) = self.pawn_store.get_mut(key) {
                pawn_ref.mirror(replicate_ref);
            }
        }
        self.queued_incoming_messages
            .push_back(ReplicateAction::ResetPawn(*key));
    }

    pub fn pawn_reset_entity(&mut self, key: &LocalEntityKey) {
        self.queued_incoming_messages
            .push_back(ReplicateAction::ResetPawnEntity(*key));
    }

    // internal

    fn replicate_delete_cleanup(&mut self, command_receiver: &mut DualCommandReceiver<T>,
                                          object_key: &LocalReplicateKey) {
        if let Some(replicate) = self.local_replicate_store.remove(&object_key) {

            if self.pawn_store.contains_key(&object_key) {
                self.pawn_store.remove(&object_key);
                let pawn_key = PawnKey::Replicate(*object_key);
                command_receiver.pawn_cleanup(&pawn_key);
            }

            self.queued_incoming_messages
                .push_back(ReplicateAction::DeleteReplicate(*object_key, replicate));
        }
    }

    fn component_delete_cleanup(&mut self, entity_key: &LocalEntityKey, component_key: &LocalComponentKey) {
        if let Some(component) = self.local_replicate_store.remove(&component_key) {
            self.queued_incoming_messages
                .push_back(ReplicateAction::RemoveComponent(*entity_key, *component_key, component));
        }
    }
}