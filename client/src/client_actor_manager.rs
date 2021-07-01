use log::warn;
use naia_shared::{
    ActorType, EventType, LocalActorKey, Manifest, PacketReader, StateMask,
};
use std::collections::{HashMap, VecDeque};

use super::client_actor_message::ClientActorMessage;
use crate::command_receiver::CommandReceiver;
use std::collections::hash_map::Keys;

#[derive(Debug)]
pub struct ClientActorManager<U: ActorType> {
    local_actor_store: HashMap<LocalActorKey, U>,
    queued_incoming_messages: VecDeque<ClientActorMessage>,
    pawn_store: HashMap<LocalActorKey, U>,
}

impl<U: ActorType> ClientActorManager<U> {
    pub fn new() -> Self {
        ClientActorManager {
            queued_incoming_messages: VecDeque::new(),
            local_actor_store: HashMap::new(),
            pawn_store: HashMap::new(),
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
                    // Creation
                    let naia_id: u16 = reader.read_u16();
                    let local_key: u16 = reader.read_u16();

                    match manifest.create_actor(naia_id, reader) {
                        Some(new_actor) => {
                            if self.local_actor_store.contains_key(&local_key) {
                                warn!("duplicate local key inserted");
                            } else {
                                //info!("creation of actor w/ key of {}", local_key);
                                self.local_actor_store.insert(local_key, new_actor);
                                self.queued_incoming_messages
                                    .push_back(ClientActorMessage::Create(local_key));
                            }
                        }
                        _ => {}
                    }
                }
                1 => {
                    // Deletion
                    let local_key = reader.read_u16();
                    self.local_actor_store.remove(&local_key);

                    if self.pawn_store.contains_key(&local_key) {
                        self.pawn_store.remove(&local_key);
                        command_receiver.pawn_cleanup(&local_key);
                    }

                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::Delete(local_key));
                }
                2 => {
                    // Update Actor
                    let local_key = reader.read_u16();
                    if let Some(actor_ref) = self.local_actor_store.get_mut(&local_key) {
                        let state_mask: StateMask = StateMask::read(reader);
                        actor_ref.read_partial(&state_mask, reader, packet_index);

                        if self.pawn_store.contains_key(&local_key) {
                            // Actor is a Pawn
                            command_receiver.replay_commands(packet_tick, local_key);

                            // remove command history until the tick that has already been checked
                            command_receiver.remove_history_until(packet_tick, local_key);
                        }
                    }

                    self.queued_incoming_messages
                        .push_back(ClientActorMessage::Update(local_key));
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
                _ => {}
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
}
