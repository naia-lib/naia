use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use slotmap::SparseSecondaryMap;

use super::{
    actor_key::actor_key::ActorKey,
    actor_record::ActorRecord,
    locality_status::LocalActorStatus,
    entity_record::EntityRecord,
    mut_handler::MutHandler,
    server_actor_message::ServerActorMessage,
};
use naia_shared::{Actor, ActorNotifiable, ActorType, KeyGenerator, LocalActorKey, Ref, StateMask, EntityKey, LocalEntityKey};

/// Manages Actors for a given Client connection and keeps them in sync on the
/// Client
#[derive(Debug)]
pub struct ServerActorManager<T: ActorType> {
    address: SocketAddr,
    local_actor_store: SparseSecondaryMap<ActorKey, Ref<dyn Actor<T>>>,
    local_to_global_key_map: HashMap<LocalActorKey, ActorKey>,
    actor_records: SparseSecondaryMap<ActorKey, ActorRecord>,
    queued_messages: VecDeque<ServerActorMessage<T>>,
    sent_messages: HashMap<u16, Vec<ServerActorMessage<T>>>,
    sent_updates: HashMap<u16, HashMap<ActorKey, Ref<StateMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler>,
    last_popped_state_mask: StateMask,
    pawn_store: HashSet<ActorKey>,
    actor_key_generator: KeyGenerator,
    entity_key_generator: KeyGenerator,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, EntityKey>,
    local_entity_store: HashMap<EntityKey, EntityRecord>,
    pawn_entity_store: HashSet<EntityKey>,
}

impl<T: ActorType> ServerActorManager<T> {
    /// Create a new ServerActorManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler>) -> Self {
        ServerActorManager {
            address,
            local_actor_store: SparseSecondaryMap::new(),
            local_to_global_key_map: HashMap::new(),
            actor_records: SparseSecondaryMap::new(),
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ActorKey, Ref<StateMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_state_mask: StateMask::new(0),
            pawn_store: HashSet::new(),
            actor_key_generator: KeyGenerator::new(),
            entity_key_generator: KeyGenerator::new(),
            local_to_global_entity_key_map: HashMap::new(),
            local_entity_store: HashMap::new(),
            pawn_entity_store: HashSet::new(),
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<ServerActorMessage<T>> {
        match self.queued_messages.pop_front() {
            Some(message) => {
                if !self.sent_messages.contains_key(&packet_index) {
                    let sent_messages_list: Vec<ServerActorMessage<T>> = Vec::new();
                    self.sent_messages.insert(packet_index, sent_messages_list);
                }

                if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
                    sent_messages_list.push(message.clone());
                }

                //clear state mask of actor if need be
                match &message {
                    ServerActorMessage::CreateActor(global_key, _, _) => {
                        if let Some(record) = self.actor_records.get(*global_key) {
                            self.last_popped_state_mask = record.get_state_mask().borrow().clone();
                        }
                        self.mut_handler
                            .borrow_mut()
                            .clear_state(&self.address, global_key);
                    }
                    ServerActorMessage::UpdateActor(global_key, local_key, state_mask, actor) => {
                        let locked_state_mask =
                            self.process_actor_update(packet_index, global_key, state_mask);
                        // return new Update message to be written
                        return Some(ServerActorMessage::UpdateActor(
                            *global_key,
                            *local_key,
                            locked_state_mask,
                            actor.clone(),
                        ));
                    }
                    ServerActorMessage::UpdatePawn(global_key, local_key, state_mask, actor) => {
                        let locked_state_mask =
                            self.process_actor_update(packet_index, global_key, state_mask);
                        // return new Update message to be written
                        return Some(ServerActorMessage::UpdatePawn(
                            *global_key,
                            *local_key,
                            locked_state_mask,
                            actor.clone(),
                        ));
                    }
                    _ => {}
                }

                return Some(message);
            }
            None => {
                return None;
            }
        }
    }

    fn process_actor_update(
        &mut self,
        packet_index: u16,
        global_key: &ActorKey,
        state_mask: &Ref<StateMask>,
    ) -> Ref<StateMask> {
        // previously the state mask was the CURRENT state mask for the actor,
        // we want to lock that in so we know exactly what we're writing
        let locked_state_mask = Ref::new(state_mask.borrow().clone());

        // place state mask in a special transmission record - like map
        if !self.sent_updates.contains_key(&packet_index) {
            let sent_updates_map: HashMap<ActorKey, Ref<StateMask>> = HashMap::new();
            self.sent_updates.insert(packet_index, sent_updates_map);
            self.last_last_update_packet_index = self.last_update_packet_index;
            self.last_update_packet_index = packet_index;
        }

        if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
            sent_updates_map.insert(*global_key, locked_state_mask.clone());
        }

        // having copied the state mask for this update, clear the state
        self.last_popped_state_mask = state_mask.borrow().clone();
        self.mut_handler
            .borrow_mut()
            .clear_state(&self.address, global_key);

        locked_state_mask
    }

    pub fn unpop_outgoing_message(&mut self, packet_index: u16, message: &ServerActorMessage<T>) {
        info!("unpopping");
        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.pop();
            if sent_messages_list.len() == 0 {
                self.sent_messages.remove(&packet_index);
            }
        }

        match &message {
            ServerActorMessage::CreateActor(global_key, _, _) => {
                self.mut_handler.borrow_mut().set_state(
                    &self.address,
                    global_key,
                    &self.last_popped_state_mask,
                );
            }
            ServerActorMessage::UpdateActor(global_key, local_key, _, actor) => {
                let original_state_mask = self.undo_actor_update(&packet_index, &global_key);
                let cloned_message = ServerActorMessage::UpdateActor(
                    *global_key,
                    *local_key,
                    original_state_mask,
                    actor.clone(),
                );
                self.queued_messages.push_front(cloned_message);
                return;
            }
            ServerActorMessage::UpdatePawn(global_key, local_key, _, actor) => {
                let original_state_mask = self.undo_actor_update(&packet_index, &global_key);
                let cloned_message = ServerActorMessage::UpdatePawn(
                    *global_key,
                    *local_key,
                    original_state_mask,
                    actor.clone(),
                );
                self.queued_messages.push_front(cloned_message);
                return;
            }
            _ => {}
        }

        self.queued_messages.push_front(message.clone());
    }

    fn undo_actor_update(&mut self, packet_index: &u16, global_key: &ActorKey) -> Ref<StateMask> {
        if let Some(sent_updates_map) = self.sent_updates.get_mut(packet_index) {
            sent_updates_map.remove(global_key);
            if sent_updates_map.len() == 0 {
                self.sent_updates.remove(&packet_index);
            }
        }

        self.last_update_packet_index = self.last_last_update_packet_index;
        self.mut_handler.borrow_mut().set_state(
            &self.address,
            global_key,
            &self.last_popped_state_mask,
        );

        self.actor_records
            .get(*global_key)
            .expect("uh oh, we don't have enough info to unpop the message")
            .get_state_mask()
            .clone()
    }

    pub fn has_actor(&self, key: &ActorKey) -> bool {
        return self.local_actor_store.contains_key(*key);
    }

    pub fn add_actor(&mut self, key: &ActorKey, actor: &Ref<dyn Actor<T>>) {
        if !self.local_actor_store.contains_key(*key) {
            self.local_actor_store.insert(*key, actor.clone());
            let local_key: LocalActorKey = self.actor_key_generator.generate();
            self.local_to_global_key_map.insert(local_key, *key);
            let state_mask_size = actor.borrow().get_state_mask_size();
            let actor_record = ActorRecord::new(local_key, state_mask_size);
            self.mut_handler.borrow_mut().register_mask(
                &self.address,
                &key,
                actor_record.get_state_mask(),
            );
            self.actor_records.insert(*key, actor_record);
            self.queued_messages
                .push_back(ServerActorMessage::CreateActor(
                    *key,
                    local_key,
                    actor.clone(),
                ));

            // if this is a pawn, send a "assign pawn" follow-up message
            if self.pawn_store.contains(key) {
                self.queued_messages
                    .push_back(ServerActorMessage::AssignPawn(*key, local_key));
            }
        }
    }

    pub fn remove_actor(&mut self, key: &ActorKey) {
        if let Some(actor_record) = self.actor_records.get_mut(*key) {
            if actor_record.status != LocalActorStatus::Deleting {
                actor_record.status = LocalActorStatus::Deleting;

                // if this is a pawn, send an "unassign pawn" message first
                if self.pawn_store.contains(key) {
                    self.queued_messages
                        .push_back(ServerActorMessage::UnassignPawn(
                            *key,
                            actor_record.local_key,
                        ));
                }

                self.queued_messages
                    .push_back(ServerActorMessage::DeleteActor(
                        *key,
                        actor_record.local_key,
                    ));
            }
        }
    }

    pub fn has_pawn(&self, key: &ActorKey) -> bool {
        return self.pawn_store.contains(key);
    }

    pub fn add_pawn(&mut self, key: &ActorKey) {
        if !self.pawn_store.contains(key) {
            self.pawn_store.insert(*key);
            if let Some(actor_record) = self.actor_records.get_mut(*key) {
                self.queued_messages
                    .push_back(ServerActorMessage::AssignPawn(*key, actor_record.local_key));
            }
        }
    }

    pub fn remove_pawn(&mut self, key: &ActorKey) {
        if self.pawn_store.contains(key) {
            self.pawn_store.remove(key);
            if let Some(actor_record) = self.actor_records.get_mut(*key) {
                self.queued_messages
                    .push_back(ServerActorMessage::UnassignPawn(
                        *key,
                        actor_record.local_key,
                    ));
            }
        }
    }

    pub fn get_global_key_from_local(&self, local_key: LocalActorKey) -> Option<&ActorKey> {
        return self.local_to_global_key_map.get(&local_key);
    }

    pub fn get_local_key_from_global(&self, global_key: &ActorKey) -> Option<LocalActorKey> {
        if let Some(record) = self.actor_records.get(*global_key) {
            return Some(record.local_key);
        }
        return None;
    }

    pub fn actor_is_created(&self, local_key: &LocalActorKey) -> bool {
        if let Some(global_key) = self.local_to_global_key_map.get(&local_key) {
            if let Some(record) = self.actor_records.get(*global_key) {
                return record.status == LocalActorStatus::Created;
            }
        }

        return false;
    }

    pub fn collect_actor_updates(&mut self) {
        for (key, record) in self.actor_records.iter() {
            if record.status == LocalActorStatus::Created
                && !record.get_state_mask().borrow().is_clear()
            {
                if let Some(actor_ref) = self.local_actor_store.get(key) {
                    if self.pawn_store.contains(&key) {
                        // handle as a pawn
                        self.queued_messages
                            .push_back(ServerActorMessage::UpdatePawn(
                                key,
                                record.local_key,
                                record.get_state_mask().clone(),
                                actor_ref.clone(),
                            ));
                    } else {
                        // handle as an actor
                        self.queued_messages
                            .push_back(ServerActorMessage::UpdateActor(
                                key,
                                record.local_key,
                                record.get_state_mask().clone(),
                                actor_ref.clone(),
                            ));
                    }
                }
            }
        }
    }

    // Entity management methods

    pub fn has_entity(&self, key: &EntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn add_entity(&mut self, key: &EntityKey) {
        if !self.local_entity_store.contains_key(key) {
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map.insert(local_key, *key);
            let entity_record = EntityRecord::new(local_key);
            self.local_entity_store.insert(*key, entity_record);
            self.queued_messages
                .push_back(ServerActorMessage::CreateEntity(
                    *key,
                    local_key
                ));

            // if this is a pawn, send a "assign pawn" follow-up message
            if self.pawn_entity_store.contains(key) {
                self.queued_messages
                    .push_back(ServerActorMessage::AssignPawnEntity(*key, local_key));
            }
        }
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        if let Some(entity_record) = self.local_entity_store.get_mut(key) {
            if entity_record.status != LocalActorStatus::Deleting {
                entity_record.status = LocalActorStatus::Deleting;

                // if this is a pawn, send an "unassign pawn" message first
                if self.pawn_entity_store.contains(key) {
                    self.queued_messages
                        .push_back(ServerActorMessage::UnassignPawnEntity(
                            *key,
                            entity_record.local_key,
                        ));
                }

                self.queued_messages
                    .push_back(ServerActorMessage::DeleteEntity(
                        *key,
                        entity_record.local_key,
                    ));
            }
        }
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.pawn_entity_store.contains(key);
    }

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        if !self.pawn_entity_store.contains(key) {
            self.pawn_entity_store.insert(*key);
            if let Some(entity_record) = self.local_entity_store.get_mut(key) {
                self.queued_messages
                    .push_back(ServerActorMessage::AssignPawnEntity(*key, entity_record.local_key));
            }
        }
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        if self.pawn_entity_store.contains(key) {
            self.pawn_entity_store.remove(key);
            if let Some(entity_record) = self.local_entity_store.get_mut(key) {
                self.queued_messages
                    .push_back(ServerActorMessage::UnassignPawnEntity(
                        *key,
                        entity_record.local_key,
                    ));
            }
        }
    }
}

impl<T: ActorType> ActorNotifiable for ServerActorManager<T> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        if let Some(delivered_messages_list) = self.sent_messages.get(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {
                match delivered_message {
                    ServerActorMessage::CreateActor(global_key, _, _) => {
                        if let Some(actor_record) = self.actor_records.get_mut(*global_key) {
                            // update actor record status
                            actor_record.status = LocalActorStatus::Created;
                        }
                    }
                    ServerActorMessage::DeleteActor(global_key_ref, local_key) => {
                        let global_key = *global_key_ref;
                        if let Some(_) = self.actor_records.get(global_key) {
                            // actually delete the actor from local records
                            self.mut_handler
                                .borrow_mut()
                                .deregister_mask(&self.address, global_key_ref);
                            self.local_actor_store.remove(global_key);
                            self.local_to_global_key_map.remove(local_key);
                            self.actor_key_generator.recycle_key(local_key);
                            self.actor_records.remove(global_key);
                            self.pawn_store.remove(&global_key);
                        }
                    }
                    ServerActorMessage::UpdateActor(_, _, _, _)
                    | ServerActorMessage::UpdatePawn(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                    ServerActorMessage::AssignPawn(_, _) => {}
                    ServerActorMessage::UnassignPawn(_, _) => {}
                    ServerActorMessage::CreateEntity(global_key, _) => {
                        if let Some(entity_record) = self.local_entity_store.get_mut(global_key) {
                            // update entity record status
                            entity_record.status = LocalActorStatus::Created;
                        }
                    }
                    ServerActorMessage::DeleteEntity(global_key, local_key) => {
                        if let Some(_) = self.local_entity_store.get(global_key) {
                            // actually delete the entity from local records
                            self.local_entity_store.remove(global_key);
                            self.local_to_global_entity_key_map.remove(local_key);
                            self.entity_key_generator.recycle_key(local_key);
                            self.pawn_entity_store.remove(&global_key);
                        }
                    }
                    ServerActorMessage::AssignPawnEntity(_, _) => {}
                    ServerActorMessage::UnassignPawnEntity(_, _) => {}
                }
            }

            self.sent_messages.remove(&packet_index);
        }
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&dropped_packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                match dropped_message {
                    // gauranteed delivery messages
                    ServerActorMessage::CreateActor(_, _, _)
                    | ServerActorMessage::DeleteActor(_, _)
                    | ServerActorMessage::AssignPawn(_, _)
                    | ServerActorMessage::UnassignPawn(_, _)
                    | ServerActorMessage::CreateEntity(_, _)
                    | ServerActorMessage::DeleteEntity(_, _)
                    | ServerActorMessage::AssignPawnEntity(_, _)
                    | ServerActorMessage::UnassignPawnEntity(_, _)=> {
                        self.queued_messages.push_back(dropped_message.clone());
                    }
                    // non-gauranteed delivery messages
                    ServerActorMessage::UpdateActor(global_key, _, _, _)
                    | ServerActorMessage::UpdatePawn(global_key, _, _, _) => {
                        if let Some(state_mask_map) = self.sent_updates.get(&dropped_packet_index) {
                            if let Some(state_mask) = state_mask_map.get(global_key) {
                                let mut new_state_mask = state_mask.borrow().clone();

                                // walk from dropped packet up to most recently sent packet
                                if dropped_packet_index != self.last_update_packet_index {
                                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                                    while packet_index != self.last_update_packet_index {
                                        if let Some(state_mask_map) =
                                            self.sent_updates.get(&packet_index)
                                        {
                                            if let Some(state_mask) = state_mask_map.get(global_key)
                                            {
                                                new_state_mask.nand(state_mask.borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                if let Some(record) = self.actor_records.get_mut(*global_key) {
                                    let mut current_state_mask =
                                        record.get_state_mask().borrow_mut();
                                    current_state_mask.or(new_state_mask.borrow());
                                }
                            }
                        }
                    }
                }
            }

            self.sent_updates.remove(&dropped_packet_index);
            self.sent_messages.remove(&dropped_packet_index);
        }
    }
}
