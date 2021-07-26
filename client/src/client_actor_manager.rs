
use std::collections::{HashSet, HashMap, VecDeque, hash_map::Keys};

use log::warn;

use naia_shared::{ActorType, EventType, LocalActorKey, Manifest, PacketReader, StateMask, LocalEntityKey, ActorMessageType, NaiaKey, LocalComponentKey, PawnKey};

use super::client_actor_message::ClientActorMessage;
use crate::dual_command_receiver::DualCommandReceiver;

#[derive(Debug)]
pub struct ClientActorManager<U: ActorType> {
    local_actor_store:                  HashMap<LocalActorKey, U>,
    queued_incoming_messages:           VecDeque<ClientActorMessage>,
    pawn_store:                         HashMap<LocalActorKey, U>,
    local_entity_store:                 HashMap<LocalEntityKey, HashSet<LocalComponentKey>>,
    pawn_entity_store:                  HashSet<LocalEntityKey>,
    component_entity_map:               HashMap<LocalComponentKey, LocalEntityKey>,
}

impl<U: ActorType> ClientActorManager<U> {
    pub fn new() -> Self {
        ClientActorManager {
            queued_incoming_messages:   VecDeque::new(),
            local_actor_store:          HashMap::new(),
            pawn_store:                 HashMap::new(),
            local_entity_store:         HashMap::new(),
            pawn_entity_store:          HashSet::new(),
            component_entity_map:       HashMap::new(),
        }
    }

    pub fn process_data<T: EventType>(
        &mut self,
        manifest: &Manifest<T, U>,
        command_receiver: &mut DualCommandReceiver<T>,
        packet_tick: u16,
        packet_index: u16,
        reader: &mut PacketReader,
    ) {
        let actor_message_count = reader.read_u8();

        for _ in 0..actor_message_count {
            let message_type = ActorMessageType::from_u8(reader.read_u8());

            match message_type {
                ActorMessageType::CreateActor => {
                    // Actor Creation
                    let naia_id: u16 = reader.read_u16();
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());

                    if let Some(new_actor) = manifest.create_actor(naia_id, reader) {
                        if self.local_actor_store.contains_key(&actor_key) {
                            warn!("duplicate local actor key inserted");
                        } else {

                            self.local_actor_store.insert(actor_key, new_actor);

                            if let Some(entity_key) = self.component_entity_map.get(&actor_key) {
                                // actor is a component
                                self.queued_incoming_messages
                                    .push_back(ClientActorMessage::AddComponent(*entity_key, actor_key));
                            } else {
                                // actor is an object
                                self.queued_incoming_messages
                                    .push_back(ClientActorMessage::CreateActor(actor_key));
                            }
                        }
                    }
                }
                ActorMessageType::DeleteActor => {
                    // Actor Deletion
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());
                    self.actor_delete_cleanup(command_receiver, &actor_key);
                }
                ActorMessageType::UpdateActor => {
                    // Actor Update
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&actor_key) {
                        // Actor is not a Pawn
                        let state_mask: StateMask = StateMask::read(reader);

                        actor_ref.read_partial(&state_mask, reader, packet_index);

                        if let Some(entity_key) = self.component_entity_map.get(&actor_key) {
                            // Actor is a Component

                            // if Entity is a Pawn, replay commands
                            if self.pawn_entity_store.contains(entity_key) {
                                let pawn_key = PawnKey::Entity(*entity_key);
                                command_receiver.replay_commands(packet_tick, &pawn_key);

                                // remove command history until the tick that has already been checked
                                command_receiver.remove_history_until(packet_tick, &pawn_key);
                            }

                            self.queued_incoming_messages
                                .push_back(ClientActorMessage::UpdateComponent(*entity_key, actor_key));
                        } else {
                            // Actor is an Object
                            self.queued_incoming_messages
                                .push_back(ClientActorMessage::UpdateActor(actor_key));
                        }
                    }
                }
                ActorMessageType::AssignPawn => {
                    // Assign Pawn
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&actor_key) {
                        self.pawn_store
                            .insert(actor_key, actor_ref.inner_ref().borrow().get_typed_copy());

                        let pawn_key = PawnKey::Actor(actor_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::AssignPawn(actor_key));
                    }
                }
                ActorMessageType::UnassignPawn => {
                    // Unassign Pawn
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());
                    if self.pawn_store.contains_key(&actor_key) {
                        self.pawn_store.remove(&actor_key);

                        let pawn_key = PawnKey::Actor(actor_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::UnassignPawn(actor_key));
                }
                ActorMessageType::UpdatePawn => {
                    // Pawn Update
                    let actor_key = LocalActorKey::from_u16(reader.read_u16());

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&actor_key) {
                        actor_ref.read_full(reader, packet_index);

                        let pawn_key = PawnKey::Actor(actor_key);

                        command_receiver.replay_commands(packet_tick, &pawn_key);

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, &pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::UpdateActor(actor_key));
                    }
                }
                ActorMessageType::CreateEntity => {
                    // Entity Creation
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.local_entity_store.contains_key(&entity_key) {
                        warn!("duplicate local entity key inserted");
                    } else {
                        self.local_entity_store.insert(entity_key, HashSet::new());
                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::CreateEntity(entity_key));
                    }
                }
                ActorMessageType::DeleteEntity => {
                    // Entity Deletion
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if let Some(component_set) = self.local_entity_store.remove(&entity_key) {

                        if self.pawn_entity_store.take(&entity_key).is_some() {
                            let pawn_key = PawnKey::Entity(entity_key);
                            command_receiver.pawn_cleanup(&pawn_key);
                        }

                        for component_key in component_set {
                            // delete all components
                            self.actor_delete_cleanup(command_receiver, &component_key);

                            self.component_entity_map.remove(&component_key);
                        }

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::DeleteEntity(entity_key));
                    } else {
                        warn!("received message attempting to delete nonexistent entity!");
                    }
                }
                ActorMessageType::AssignPawnEntity => {
                    // Assign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.local_entity_store.contains_key(&entity_key) {
                        self.pawn_entity_store
                            .insert(entity_key);

                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_init(&pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::AssignPawnEntity(entity_key));
                    }
                }
                ActorMessageType::UnassignPawnEntity => {
                    // Unassign Pawn Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if self.pawn_entity_store.contains(&entity_key) {
                        self.pawn_entity_store.remove(&entity_key);
                        let pawn_key = PawnKey::Entity(entity_key);
                        command_receiver.pawn_cleanup(&pawn_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::UnassignPawnEntity(entity_key));
                }
                ActorMessageType::AddComponent => {
                    // Add Component to Entity
                    let local_entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let local_component_key = LocalComponentKey::from_u16(reader.read_u16());
                    if let Some(component_set) = self.local_entity_store.get_mut(&local_entity_key) {
                        self.component_entity_map.insert(local_component_key, local_entity_key);
                        component_set.insert(local_component_key);
                    } else {
                        warn!("received add_component message for nonexistent entity");
                    }
                }
                ActorMessageType::Unknown => {
                    warn!("received unknown type of actor message");
                    return;
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ClientActorMessage> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn actor_keys(&self) -> Keys<LocalActorKey, U> {
        return self.local_actor_store.keys();
    }

    pub fn get_actor(&self, key: &LocalActorKey) -> Option<&U> {
        return self.local_actor_store.get(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalActorKey, U> {
        return self.pawn_store.keys();
    }

    pub fn get_pawn(&self, key: &LocalActorKey) -> Option<&U> {
        return self.pawn_store.get(key);
    }

    pub fn pawn_reset(&mut self, key: &LocalActorKey) {
        if let Some(actor_ref) = self.local_actor_store.get(key) {
            if let Some(pawn_ref) = self.pawn_store.get_mut(key) {
                pawn_ref.mirror(actor_ref);
            }
        }
        self.queued_incoming_messages
            .push_back(ClientActorMessage::ResetPawn(*key));
    }

    pub fn pawn_reset_entity(&mut self, key: &LocalEntityKey) {
        self.queued_incoming_messages
            .push_back(ClientActorMessage::ResetPawnEntity(*key));
    }

    // internal

    fn actor_delete_cleanup<T: EventType>(&mut self, command_receiver: &mut DualCommandReceiver<T>,
                                          actor_key: &LocalActorKey) {
        self.local_actor_store.remove(&actor_key);

        if self.pawn_store.contains_key(&actor_key) {
            self.pawn_store.remove(&actor_key);
            let pawn_key = PawnKey::Actor(*actor_key);
            command_receiver.pawn_cleanup(&pawn_key);
        }

        if let Some(entity_key) = self.component_entity_map.get(actor_key) {
            // actor is a component
            self.queued_incoming_messages
                .push_back(ClientActorMessage::RemoveComponent(*entity_key, *actor_key));
        } else {
            // actor is an object
            self.queued_incoming_messages
                .push_back(ClientActorMessage::DeleteActor(*actor_key));
        }
    }
}
