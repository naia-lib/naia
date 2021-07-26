
use std::collections::{HashSet, HashMap, VecDeque, hash_map::Keys};

use log::warn;

use naia_shared::{ActorType, EventType, LocalActorKey, Manifest, PacketReader, StateMask, LocalEntityKey};

use super::client_actor_message::ClientActorMessage;
use crate::command_receiver::CommandReceiver;

#[derive(Debug)]
pub struct ClientActorManager<U: ActorType> {
    local_actor_store: HashMap<LocalActorKey, U>,
    queued_incoming_messages: VecDeque<ClientActorMessage>,
    pawn_store: HashMap<LocalActorKey, U>,
    local_entity_store: HashMap<LocalEntityKey, HashSet<LocalActorKey>>,
    pawn_entity_store: HashSet<LocalEntityKey>,
    component_entity_map: HashMap<LocalActorKey, LocalEntityKey>,
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
        command_receiver: &mut CommandReceiver<T>,
        packet_tick: u16,
        packet_index: u16,
        reader: &mut PacketReader,
    ) {
        let actor_message_count = reader.read_u8();
        //info!("reading {} actor messages", actor_message_count);
        for _x in 0..actor_message_count {
            let message_type: u8 = reader.read_u8();

            match message_type {
                0 => {
                    // Actor Creation
                    let naia_id: u16 = reader.read_u16();
                    let local_actor_key: u16 = reader.read_u16();

                    if let Some(new_actor) = manifest.create_actor(naia_id, reader) {
                        if self.local_actor_store.contains_key(&local_actor_key) {
                            warn!("duplicate local actor key inserted");
                        } else {

                            self.local_actor_store.insert(local_actor_key, new_actor);

                            if let Some(entity_key) = self.component_entity_map.get(&local_actor_key) {
                                // actor is a component
                                self.queued_incoming_messages
                                    .push_back(ClientActorMessage::AddComponent(*entity_key, local_actor_key));
                            } else {
                                // actor is an object
                                self.queued_incoming_messages
                                    .push_back(ClientActorMessage::CreateActor(local_actor_key));
                            }
                        }
                    }
                }
                1 => {
                    // Actor Deletion
                    let local_key = reader.read_u16();
                    self.actor_delete_cleanup(command_receiver, &local_key);
                }
                2 => {
                    // Actor Update
                    let actor_key = reader.read_u16();

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&actor_key) {
                        // Actor is not a Pawn
                        let state_mask: StateMask = StateMask::read(reader);

                        actor_ref.read_partial(&state_mask, reader, packet_index);

                        if let Some(entity_key) = self.component_entity_map.get(&actor_key) {
                            // actor is a component
                            self.queued_incoming_messages
                            .push_back(ClientActorMessage::UpdateComponent(*entity_key, actor_key));
                        } else {
                            // actor is an object
                            self.queued_incoming_messages
                            .push_back(ClientActorMessage::UpdateActor(actor_key));
                        }
                    }
                }
                3 => {
                    // Assign Pawn
                    let local_key: u16 = reader.read_u16();

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&local_key) {
                        self.pawn_store
                            .insert(local_key, actor_ref.inner_ref().borrow().get_typed_copy());

                        command_receiver.pawn_init(&local_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::AssignPawn(local_key));
                    }
                }
                4 => {
                    // Unassign Pawn
                    let local_key: u16 = reader.read_u16();
                    if self.pawn_store.contains_key(&local_key) {
                        self.pawn_store.remove(&local_key);
                        command_receiver.pawn_cleanup(&local_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::UnassignPawn(local_key));
                }
                5 => {
                    // Pawn Update
                    let pawn_key = reader.read_u16();

                    if let Some(actor_ref) = self.local_actor_store.get_mut(&pawn_key) {
                        actor_ref.read_full(reader, packet_index);

                        command_receiver.replay_commands(packet_tick, pawn_key);

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, pawn_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::UpdateActor(pawn_key));
                    }
                }
                6 => {
                    // Entity Creation
                    let local_key = reader.read_u16();
                    if self.local_entity_store.contains_key(&local_key) {
                        warn!("duplicate local entity key inserted");
                    } else {
                        self.local_entity_store.insert(local_key, HashSet::new());
                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::CreateEntity(local_key));
                    }
                }
                7 => {
                    // Entity Deletion
                    let entity_key = reader.read_u16();
                    if let Some(component_set) = self.local_entity_store.remove(&entity_key) {

                        if self.pawn_entity_store.take(&entity_key).is_some() {
                            command_receiver.pawn_entity_cleanup(&entity_key);
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
                8 => {
                    // Assign Pawn Entity
                    let local_key: u16 = reader.read_u16();
                    if self.local_entity_store.contains_key(&local_key) {
                        self.pawn_entity_store
                            .insert(local_key);

                        command_receiver.pawn_entity_init(&local_key);

                        self.queued_incoming_messages
                            .push_back(ClientActorMessage::AssignPawnEntity(local_key));
                    }
                }
                9 => {
                    // Unassign Pawn Entity
                    let local_key: u16 = reader.read_u16();
                    if self.pawn_entity_store.contains(&local_key) {
                        self.pawn_entity_store.remove(&local_key);
                        command_receiver.pawn_entity_cleanup(&local_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::UnassignPawnEntity(local_key));
                }
                10 => {
                    // Add Component to Entity
                    let local_entity_key = reader.read_u16();
                    let local_component_key = reader.read_u16();
                    if let Some(component_set) = self.local_entity_store.get_mut(&local_entity_key) {
                        self.component_entity_map.insert(local_component_key, local_entity_key);
                        component_set.insert(local_component_key);
                    } else {
                        warn!("received add_component message for nonexistent entity");
                    }
                }
                _ => {
                    warn!("what's going on here? {}", message_type);
                    return;
                }
            }
        }
    }

    fn actor_delete_cleanup<T: EventType>(&mut self, command_receiver: &mut CommandReceiver<T>,
                                          actor_key: &LocalActorKey) {
        self.local_actor_store.remove(&actor_key);

        if self.pawn_store.contains_key(&actor_key) {
            self.pawn_store.remove(&actor_key);
            command_receiver.pawn_cleanup(&actor_key);
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
}
