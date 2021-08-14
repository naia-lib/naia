
use std::collections::{HashSet, HashMap, VecDeque, hash_map::Keys};

use log::warn;

use naia_shared::{ProtocolType, LocalObjectKey, Manifest, PacketReader, DiffMask,
                  LocalEntityKey, StateMessageType, NaiaKey, LocalComponentKey, PawnKey};

use super::{client_state_message::ClientStateMessage, dual_command_receiver::DualCommandReceiver};

#[derive(Debug)]
pub struct ClientStateManager<T: ProtocolType> {
    local_state_store:                  HashMap<LocalObjectKey, T>,
    queued_incoming_messages:           VecDeque<ClientStateMessage<T>>,
    pawn_store:                         HashMap<LocalObjectKey, T>,
    local_entity_store:                 HashMap<LocalEntityKey, HashSet<LocalComponentKey>>,
    pawn_entity_store:                  HashSet<LocalEntityKey>,
    component_entity_map:               HashMap<LocalComponentKey, LocalEntityKey>,
}

impl<T: ProtocolType> ClientStateManager<T> {
    pub fn new() -> Self {
        ClientStateManager {
            queued_incoming_messages:           VecDeque::new(),
            local_state_store:                  HashMap::new(),
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
        let state_message_count = reader.read_u8();

        for _ in 0..state_message_count {
            let message_type = StateMessageType::from_u8(reader.read_u8());

            match message_type {
                StateMessageType::CreateState => {
                    // State Creation
                    let naia_id: u16 = reader.read_u16();
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    let new_state = manifest.create_state(naia_id, reader);
                    if !self.local_state_store.contains_key(&object_key) {
                        self.local_state_store.insert(object_key, new_state);

                        self.queued_incoming_messages
                            .push_back(ClientStateMessage::CreateState(object_key));
                    } else {
                        // may have received a duplicate message
                        warn!("attempted to insert duplicate local state key");
                    }
                }
                StateMessageType::DeleteState => {
                    // State Deletion
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if self.component_entity_map.contains_key(&object_key) {
                        // State is a Component
                        let entity_key = self.component_entity_map.remove(&object_key).unwrap();
                        let component_set = self.local_entity_store.get_mut(&entity_key)
                            .expect("entity not instantiated properly?");
                        if !component_set.remove(&object_key) {
                            panic!("trying to delete non-existent component");
                        }
                        self.component_delete_cleanup(&entity_key, &object_key);
                    } else {
                        // State is a State
                        self.state_delete_cleanup(command_receiver, &object_key);
                    }
                }
                StateMessageType::UpdateState => {
                    // State Update
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if let Some(state_ref) = self.local_state_store.get_mut(&object_key) {
                        // State is not a Pawn
                        let diff_mask: DiffMask = DiffMask::read(reader);

                        state_ref.read_partial(&diff_mask, reader, packet_index);

                        if let Some(entity_key) = self.component_entity_map.get(&object_key) {
                            // State is a Component

                            // if Entity is a Pawn, replay commands
                            if self.pawn_entity_store.contains(entity_key) {
                                let pawn_key = PawnKey::Entity(*entity_key);
                                command_receiver.replay_commands(packet_tick, &pawn_key);

                                // remove command history until the tick that has already been checked
                                command_receiver.remove_history_until(packet_tick, &pawn_key);
                            }

                            self.queued_incoming_messages
                                .push_back(ClientStateMessage::UpdateComponent(*entity_key, object_key));
                        } else {
                            // State is an State
                            self.queued_incoming_messages
                                .push_back(ClientStateMessage::UpdateState(object_key));
                        }
                    }
                }
                StateMessageType::AssignPawn => {
                    // Assign Pawn
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if let Some(state_ref) = self.local_state_store.get_mut(&object_key) {
                        self.pawn_store
                            .insert(object_key, state_ref.inner_ref().borrow().get_typed_copy());

                        let pawn_key = PawnKey::State(object_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientStateMessage::AssignPawn(object_key));
                    }
                }
                StateMessageType::UnassignPawn => {
                    // Unassign Pawn
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());
                    if self.pawn_store.contains_key(&object_key) {
                        self.pawn_store.remove(&object_key);

                        let pawn_key = PawnKey::State(object_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientStateMessage::UnassignPawn(object_key));
                }
                StateMessageType::UpdatePawn => {
                    // Pawn Update
                    let object_key = LocalObjectKey::from_u16(reader.read_u16());

                    if let Some(state_ref) = self.local_state_store.get_mut(&object_key) {
                        state_ref.read_full(reader, packet_index);

                        let pawn_key = PawnKey::State(object_key);

                        command_receiver.replay_commands(packet_tick, &pawn_key);

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, &pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientStateMessage::UpdateState(object_key));
                    }
                }
                StateMessageType::CreateEntity => {
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
                            manifest.create_state(naia_id, reader);
                        }
                    } else {
                        let mut component_list: Vec<LocalComponentKey> = Vec::new();
                        let mut component_set = HashSet::new();

                        for _ in 0..components_num {
                            // Component Creation
                            let naia_id: u16 = reader.read_u16();
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_state = manifest.create_state(naia_id, reader);
                            if self.local_state_store.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                self.local_state_store.insert(component_key, new_state);
                                self.component_entity_map.insert(component_key, entity_key);
                                component_list.push(component_key);
                                component_set.insert(component_key);
                            }
                        }

                        self.local_entity_store.insert(entity_key, component_set);

                        self.queued_incoming_messages
                            .push_back(ClientStateMessage::CreateEntity(entity_key, component_list));
                    }
                }
                StateMessageType::DeleteEntity => {
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
                            .push_back(ClientStateMessage::DeleteEntity(entity_key));
                    } else {
                        // its possible we received a very late duplicate message
                        warn!("received message attempting to delete nonexistent entity: {}", entity_key.to_u16());
                    }
                }
                StateMessageType::AssignPawnEntity => {
                    // Assign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.local_entity_store.contains_key(&entity_key) {
                        self.pawn_entity_store
                            .insert(entity_key);

                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientStateMessage::AssignPawnEntity(entity_key));
                    }
                }
                StateMessageType::UnassignPawnEntity => {
                    // Unassign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.pawn_entity_store.contains(&entity_key) {
                        self.pawn_entity_store.remove(&entity_key);
                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientStateMessage::UnassignPawnEntity(entity_key));
                }
                StateMessageType::AddComponent => {
                    // Add Component to Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let naia_id: u16 = reader.read_u16();
                    let component_key = LocalObjectKey::from_u16(reader.read_u16());

                    let new_component = manifest.create_state(naia_id, reader);
                    if self.local_state_store.contains_key(&component_key) {
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
                            self.local_state_store.insert(component_key, new_component);

                            self.component_entity_map.insert(component_key, entity_key);
                            let component_set = self.local_entity_store.get_mut(&entity_key).unwrap();

                            component_set.insert(component_key);

                            self.queued_incoming_messages
                                .push_back(ClientStateMessage::AddComponent(entity_key, component_key));
                        }
                    }
                }
                StateMessageType::Unknown => {
                    panic!("received unknown type of state message");
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ClientStateMessage<T>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn object_keys(&self) -> Vec<LocalObjectKey> {
        let mut output: Vec<LocalObjectKey> = Vec::new();
        for key in self.local_state_store.keys() {
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

    pub fn get_state(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.local_state_store.get(key);
    }

    pub fn has_state(&self, key: &LocalObjectKey) -> bool {
        return self.local_state_store.contains_key(key);
    }

    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalObjectKey, T> {
        return self.pawn_store.keys();
    }

    pub fn get_pawn(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.pawn_store.get(key);
    }

    pub fn pawn_reset(&mut self, key: &LocalObjectKey) {
        if let Some(state_ref) = self.local_state_store.get(key) {
            if let Some(pawn_ref) = self.pawn_store.get_mut(key) {
                pawn_ref.mirror(state_ref);
            }
        }
        self.queued_incoming_messages
            .push_back(ClientStateMessage::ResetPawn(*key));
    }

    pub fn pawn_reset_entity(&mut self, key: &LocalEntityKey) {
        self.queued_incoming_messages
            .push_back(ClientStateMessage::ResetPawnEntity(*key));
    }

    // internal

    fn state_delete_cleanup(&mut self, command_receiver: &mut DualCommandReceiver<T>,
                                          object_key: &LocalObjectKey) {
        if let Some(state) = self.local_state_store.remove(&object_key) {

            if self.pawn_store.contains_key(&object_key) {
                self.pawn_store.remove(&object_key);
                let pawn_key = PawnKey::State(*object_key);
                command_receiver.pawn_cleanup(&pawn_key);
            }

            self.queued_incoming_messages
                .push_back(ClientStateMessage::DeleteState(*object_key, state));
        }
    }

    fn component_delete_cleanup(&mut self, entity_key: &LocalEntityKey, component_key: &LocalComponentKey) {
        if let Some(component) = self.local_state_store.remove(&component_key) {
            self.queued_incoming_messages
                .push_back(ClientStateMessage::RemoveComponent(*entity_key, *component_key, component));
        }
    }
}