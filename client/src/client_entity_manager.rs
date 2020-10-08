use byteorder::{BigEndian, ReadBytesExt};
use log::warn;
use naia_shared::{
    EntityType, EventType, LocalEntityKey, Manifest, PacketReader, SequenceBuffer,
    SequenceIterator, StateMask,
};
use std::collections::{HashMap, VecDeque};

use super::client_entity_message::ClientEntityMessage;
use crate::{command_receiver::CommandReceiver, interpolation_manager::InterpolationManager};
use std::collections::hash_map::Keys;

const PAWN_HISTORY_SIZE: u16 = 64;

#[derive(Debug)]
pub struct ClientEntityManager<U: EntityType> {
    local_entity_store: HashMap<LocalEntityKey, U>,
    queued_incoming_messages: VecDeque<ClientEntityMessage>,
    pawn_store: HashMap<LocalEntityKey, U>,
    pawn_history: HashMap<LocalEntityKey, SequenceBuffer<U>>,
}

impl<U: EntityType> ClientEntityManager<U> {
    pub fn new() -> Self {
        ClientEntityManager {
            queued_incoming_messages: VecDeque::new(),
            local_entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
            pawn_history: HashMap::new(),
        }
    }

    pub fn process_data<T: EventType>(
        &mut self,
        manifest: &Manifest<T, U>,
        command_receiver: &mut CommandReceiver<T>,
        interpolator: &mut InterpolationManager<U>,
        packet_tick: u16,
        packet_index: u16,
        reader: &mut PacketReader,
    ) {
        let entity_message_count = reader.read_u8();
        //info!("reading {} entity messages", entity_message_count);
        for _x in 0..entity_message_count {
            let message_type: u8 = reader.read_u8();

            match message_type {
                0 => {
                    // Creation
                    let naia_id: u16 = reader.read_u16();
                    let local_key: u16 = reader.read_u16();
                    let payload_length: u8 = reader.read_u8();

                    match manifest.create_entity(naia_id, reader) {
                        Some(new_entity) => {
                            if self.local_entity_store.contains_key(&local_key) {
                                warn!("duplicate local key inserted");
                            } else {
                                //info!("creation of entity w/ key of {}", local_key);
                                let is_interpolated = new_entity.is_interpolated();
                                self.local_entity_store.insert(local_key, new_entity);
                                if is_interpolated {
                                    interpolator.create_interpolation(&self, &local_key);
                                }
                                self.queued_incoming_messages
                                    .push_back(ClientEntityMessage::Create(local_key));
                            }
                        }
                        _ => {}
                    }
                }
                1 => {
                    // Deletion
                    let local_key = reader.read_u16();
                    self.local_entity_store.remove(&local_key);
                    interpolator.delete_interpolation(&local_key);

                    if self.pawn_store.contains_key(&local_key) {
                        self.pawn_store.remove(&local_key);
                        self.pawn_history.remove(&local_key);
                        command_receiver.pawn_cleanup(&local_key);
                        interpolator.delete_pawn_interpolation(&local_key);
                    }

                    self.queued_incoming_messages
                        .push_back(ClientEntityMessage::Delete(local_key));
                }
                2 => {
                    // Update Entity
                    let local_key = reader.read_u16();

                    if let Some(entity_ref) = self.local_entity_store.get_mut(&local_key) {
                        // Entity is not a Pawn
                        let state_mask: StateMask = StateMask::read(reader);
                        let payload_length: u8 = reader.read_u8();

                        entity_ref.read_partial(&state_mask, reader, packet_index);

                        self.queued_incoming_messages
                            .push_back(ClientEntityMessage::Update(local_key));
                    }
                }
                3 => {
                    // Assign Pawn
                    let local_key: u16 = reader.read_u16();

                    if let Some(entity_ref) = self.local_entity_store.get_mut(&local_key) {
                        self.pawn_store.insert(
                            local_key,
                            entity_ref.inner_ref().as_ref().borrow().get_typed_copy(),
                        );

                        self.pawn_history
                            .insert(local_key, SequenceBuffer::with_capacity(PAWN_HISTORY_SIZE));

                        command_receiver.pawn_init(&local_key);

                        if entity_ref.is_interpolated() {
                            interpolator.create_pawn_interpolation(&self, &local_key);
                        }

                        self.queued_incoming_messages
                            .push_back(ClientEntityMessage::AssignPawn(local_key));
                    }
                }
                4 => {
                    // Unassign Pawn
                    let local_key: u16 = reader.read_u16();
                    if self.pawn_store.contains_key(&local_key) {
                        self.pawn_store.remove(&local_key);
                        self.pawn_history.remove(&local_key);
                        command_receiver.pawn_cleanup(&local_key);
                        interpolator.delete_pawn_interpolation(&local_key);
                    }
                    self.queued_incoming_messages
                        .push_back(ClientEntityMessage::UnassignPawn(local_key));
                }
                5 => {
                    // Update Pawn
                    let local_key = reader.read_u16();

                    if let Some(entity_ref) = self.local_entity_store.get_mut(&local_key) {
                        let payload_length: u8 = reader.read_u8();

                        entity_ref.read_full(reader, packet_index);

                        // check it against it's history
                        if let Some(pawn_history) = self.pawn_history.get_mut(&local_key) {
                            if let Some(historical_pawn) = pawn_history.get(packet_tick) {
                                if !entity_ref.equals_prediction(historical_pawn) {
                                    // prediction error encountered!
                                    command_receiver.replay_commands(packet_tick, local_key);
                                } else {
                                    pawn_history.remove_until(packet_tick);
                                }
                            }
                        }

                        // remove command history until the tick that has already been checked
                        command_receiver.remove_history_until(packet_tick, local_key);

                        self.queued_incoming_messages
                            .push_back(ClientEntityMessage::Update(local_key));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ClientEntityMessage> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn entity_keys(&self) -> Keys<LocalEntityKey, U> {
        return self.local_entity_store.keys();
    }

    pub fn get_entity(&self, key: &LocalEntityKey) -> Option<&U> {
        return self.local_entity_store.get(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalEntityKey, U> {
        return self.pawn_store.keys();
    }

    pub fn pawn_history_iter(&self, key: &LocalEntityKey) -> Option<SequenceIterator<'_, U>> {
        if let Some(pawn_history) = self.pawn_history.get(&key) {
            return Some(pawn_history.iter());
        }
        return None;
    }

    pub fn get_pawn(&self, key: &LocalEntityKey) -> Option<&U> {
        return self.pawn_store.get(key);
    }

    pub fn pawn_reset(&mut self, key: &LocalEntityKey) {
        if let Some(entity_ref) = self.local_entity_store.get_mut(key) {
            self.pawn_store.remove(key);
            self.pawn_store.insert(
                *key,
                entity_ref.inner_ref().as_ref().borrow().get_typed_copy(),
            );
        }
    }

    pub fn pawn_clear_history(&mut self, key: &LocalEntityKey) {
        if let Some(pawn_history) = self.pawn_history.get_mut(&key) {
            pawn_history.clear();
        }
    }

    pub fn save_replay_snapshot(&mut self, history_tick: u16, pawn_key: &LocalEntityKey) {
        if let Some(pawn_ref) = self.pawn_store.get(pawn_key) {
            if let Some(pawn_history) = self.pawn_history.get_mut(pawn_key) {
                pawn_history.insert(
                    history_tick,
                    pawn_ref.inner_ref().as_ref().borrow().get_typed_copy(),
                );
            }
        }
    }
}
