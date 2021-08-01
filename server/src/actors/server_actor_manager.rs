use std::{
    borrow::Borrow,
    clone::Clone,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
};

use slotmap::SparseSecondaryMap;

use naia_shared::{Actor, ActorNotifiable, ActorType, KeyGenerator, LocalActorKey, Ref, StateMask, EntityKey, LocalEntityKey};

use super::{
    actor_key::{actor_key::ActorKey, ComponentKey},
    actor_record::ActorRecord,
    locality_status::LocalityStatus,
    entity_record::EntityRecord,
    mut_handler::MutHandler,
    server_actor_message::ServerActorMessage,
};

use byteorder::{BigEndian, WriteBytesExt};

use naia_shared::{EventType, Manifest, MTU_SIZE, NaiaKey};

use crate::server_packet_writer::ServerPacketWriter;

/// Manages Actors/Entities for a given Client connection and keeps them in sync on the
/// Client
#[derive(Debug)]
pub struct ServerActorManager<T: ActorType> {
    address: SocketAddr,
    // actors
    actor_key_generator: KeyGenerator<LocalActorKey>,
    local_actor_store: SparseSecondaryMap<ActorKey, Ref<dyn Actor<T>>>,
    local_to_global_key_map: HashMap<LocalActorKey, ActorKey>,
    actor_records: SparseSecondaryMap<ActorKey, ActorRecord>,
    pawn_store: HashSet<ActorKey>,
    delayed_actor_deletions: HashSet<ActorKey>,
    // entities
    entity_key_generator: KeyGenerator<LocalEntityKey>,
    local_entity_store: HashMap<EntityKey, EntityRecord>,
    local_to_global_entity_key_map: HashMap<LocalEntityKey, EntityKey>,
    pawn_entity_store: HashSet<EntityKey>,
    delayed_entity_deletions: HashSet<EntityKey>,
    // messages / updates / ect
    queued_messages: VecDeque<ServerActorMessage<T>>,
    sent_messages: HashMap<u16, Vec<ServerActorMessage<T>>>,
    sent_updates: HashMap<u16, HashMap<ActorKey, Ref<StateMask>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Ref<MutHandler>,
    last_popped_state_mask: Option<StateMask>,
    last_popped_state_mask_list: Option<Vec<(ActorKey, StateMask)>>,
}

impl<T: ActorType> ServerActorManager<T> {
    /// Create a new ServerActorManager, given the client's address and a
    /// reference to a MutHandler associated with the Client
    pub fn new(address: SocketAddr, mut_handler: &Ref<MutHandler>) -> Self {
        ServerActorManager {
            address,
            // actors
            actor_key_generator: KeyGenerator::new(),
            local_actor_store: SparseSecondaryMap::new(),
            local_to_global_key_map: HashMap::new(),
            actor_records: SparseSecondaryMap::new(),
            pawn_store: HashSet::new(),
            delayed_actor_deletions: HashSet::new(),
            // entities
            entity_key_generator: KeyGenerator::new(),
            local_to_global_entity_key_map: HashMap::new(),
            local_entity_store: HashMap::new(),
            pawn_entity_store: HashSet::new(),
            delayed_entity_deletions: HashSet::new(),
            // messages / updates / ect
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<ActorKey, Ref<StateMask>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_state_mask: None,
            last_popped_state_mask_list: None,
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<ServerActorMessage<T>> {
        let queued_message_opt = self.queued_messages.pop_front();
        if queued_message_opt.is_none() {
            return None;
        }
        let mut message = queued_message_opt.unwrap();

        let replacement_message: Option<ServerActorMessage<T>> = {
            match &message {
                ServerActorMessage::CreateEntity(global_entity_key, local_entity_key, _) => {
                    let mut component_list = Vec::new();

                    let entity_record = self.local_entity_store.get(global_entity_key)
                        .expect("trying to pop an actor message for an entity which has not been initialized correctly");

                    let components: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                    for global_component_key in components {
                        let component_ref = self.local_actor_store.get(*global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        let component_record = self.actor_records.get(*global_component_key)
                            .expect("trying to initiate a component which has not been initialized correctly");
                        component_list.push((*global_component_key, component_record.local_key, component_ref.clone()));
                    }

                    Some(ServerActorMessage::CreateEntity(*global_entity_key, *local_entity_key, Some(component_list)))
                }
                _ => None
            }
        };

        if let Some(new_message) = replacement_message {
            message = new_message;
        }

        if !self.sent_messages.contains_key(&packet_index) {
            self.sent_messages.insert(packet_index, Vec::new());
        }

        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.push(message.clone());
        }

        //clear state mask of actor if need be
        match &message {
            ServerActorMessage::CreateActor(global_key, _, _) => {
                self.pop_create_actor_state_mask(global_key);
            }
            ServerActorMessage::AddComponent(_, global_key, _, _) => {
                self.pop_create_actor_state_mask(global_key);
            }
            ServerActorMessage::CreateEntity(_, _, components_list_opt) => {
                if let Some(components_list) = components_list_opt {
                    let mut state_mask_list: Vec<(ComponentKey, StateMask)> = Vec::new();
                    for (global_component_key, _, _) in components_list {
                        if let Some(record) = self.actor_records.get(*global_component_key) {
                            state_mask_list.push((*global_component_key, record.get_state_mask().borrow().clone()));
                        }
                        self.mut_handler
                            .borrow_mut()
                            .clear_state(&self.address, global_component_key);
                    }
                    self.last_popped_state_mask_list = Some(state_mask_list);
                }
            }
            ServerActorMessage::UpdateActor(global_key, local_key, state_mask, actor) => {
                return Some(self.pop_update_actor_state_mask(false, packet_index, global_key, local_key, state_mask, actor));
            }
            ServerActorMessage::UpdatePawn(global_key, local_key, state_mask, actor) => {
                return Some(self.pop_update_actor_state_mask(true, packet_index, global_key, local_key, state_mask, actor));
            }
            _ => {}
        }

        return Some(message);
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
                self.unpop_create_actor_state_mask(global_key);
            }
            ServerActorMessage::AddComponent(_, global_key, _, _) => {
                self.unpop_create_actor_state_mask(global_key);
            }
            ServerActorMessage::CreateEntity(_, _, _) => {
                if let Some(last_popped_state_mask_list) = &self.last_popped_state_mask_list {
                    for (global_component_key, last_popped_state_mask) in last_popped_state_mask_list {
                        self.mut_handler.borrow_mut().set_state(
                            &self.address,
                            global_component_key,
                            &last_popped_state_mask,
                        );
                    }
                }
            }
            ServerActorMessage::UpdateActor(global_key, local_key, _, actor) => {
                let cloned_message = self.unpop_update_actor_state_mask(false, packet_index, global_key, local_key, actor);
                self.queued_messages.push_front(cloned_message);
                return;
            }
            ServerActorMessage::UpdatePawn(global_key, local_key, _, actor) => {
                let cloned_message = self.unpop_update_actor_state_mask(true, packet_index, global_key, local_key, actor);
                self.queued_messages.push_front(cloned_message);
                return;
            }
            _ => {}
        }

        self.queued_messages.push_front(message.clone());
    }

    // Actors

    pub fn add_actor(&mut self, key: &ActorKey, actor: &Ref<dyn Actor<T>>) {
        let local_key = self.actor_init(key, actor, LocalityStatus::Creating);

        self.queued_messages
            .push_back(ServerActorMessage::CreateActor(
                *key,
                local_key,
                actor.clone(),
            ));
    }

    pub fn remove_actor(&mut self, key: &ActorKey) {

        if self.has_pawn(key) {
            self.remove_pawn(key);
        }

        if let Some(actor_record) = self.actor_records.get_mut(*key) {
            match actor_record.status {
                LocalityStatus::Creating => {
                    // queue deletion message to be sent after creation
                    self.delayed_actor_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion message
                    actor_delete(&mut self.queued_messages, actor_record, key);
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        } else {
            panic!("attempting to remove an actor from a connection within which it does not exist");
        }
    }

    pub fn has_actor(&self, key: &ActorKey) -> bool {
        return self.local_actor_store.contains_key(*key);
    }

    // Pawns

    pub fn add_pawn(&mut self, key: &ActorKey) {
        if self.local_actor_store.contains_key(*key) {
            if !self.pawn_store.contains(key) {
                self.pawn_store.insert(*key);
                if let Some(actor_record) = self.actor_records.get_mut(*key) {
                    self.queued_messages
                        .push_back(ServerActorMessage::AssignPawn(*key, actor_record.local_key));
                }
            }
        } else {
            panic!("user connection does not have local actor to make into a pawn!");
        }
    }

    pub fn remove_pawn(&mut self, key: &ActorKey) {
        if self.pawn_store.remove(key) {
            if let Some(actor_record) = self.actor_records.get_mut(*key) {
                self.queued_messages
                    .push_back(ServerActorMessage::UnassignPawn(
                        *key,
                        actor_record.local_key,
                    ));
            }
        } else {
            panic!("attempt to unassign a pawn actor from a connection to which it is not assigned as a pawn in the first place")
        }
    }

    pub fn has_pawn(&self, key: &ActorKey) -> bool {
        return self.pawn_store.contains(key);
    }

    // Entities

    pub fn add_entity(&mut self, global_key: &EntityKey,
                      components_ref: &Ref<HashSet<ComponentKey>>,
                      component_list: &Vec<(ComponentKey, Ref<dyn Actor<T>>)>) {
        if !self.local_entity_store.contains_key(global_key) {
            // first, add components
            for (component_key, component_ref) in component_list {
                self.actor_init(component_key, component_ref, LocalityStatus::Creating);
            }

            // then, add entity
            let local_key: LocalEntityKey = self.entity_key_generator.generate();
            self.local_to_global_entity_key_map.insert(local_key, *global_key);
            let entity_record = EntityRecord::new(local_key, components_ref);
            self.local_entity_store.insert(*global_key, entity_record);
            self.queued_messages
                .push_back(ServerActorMessage::CreateEntity(
                    *global_key,
                    local_key,
                    None,
                ));
        } else {
            panic!("added entity twice");
        }
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        if self.has_pawn_entity(key) {
            self.remove_pawn_entity(key);
        }

        if let Some(entity_record) = self.local_entity_store.get_mut(key) {

            match entity_record.status {
                LocalityStatus::Creating => {
                    // queue deletion message to be sent after creation
                    self.delayed_entity_deletions.insert(*key);
                }
                LocalityStatus::Created => {
                    // send deletion message
                    entity_delete(&mut self.queued_messages, entity_record, key);

                    // Entity deletion IS Component deletion, so update those actor records accordingly
                    let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                    for component_key in component_set {
                        self.pawn_store.remove(component_key);

                        if let Some(actor_record) = self.actor_records.get_mut(*component_key) {
                            actor_record.status = LocalityStatus::Deleting;
                        }
                    }
                }
                LocalityStatus::Deleting => {
                    // deletion in progress, do nothing
                }
            }
        }
    }

    pub fn has_entity(&self, key: &EntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    // Pawn Entities

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        if self.local_entity_store.contains_key(key) {
            if !self.pawn_entity_store.contains(key) {
                self.pawn_entity_store.insert(*key);
                let local_key = self.local_entity_store.get(key)
                    .unwrap()
                    .local_key;
                self.queued_messages
                    .push_back(ServerActorMessage::AssignPawnEntity(*key, local_key));
            } else {
                warn!("attempting to assign a pawn entity twice");
            }
        } else {
            warn!("attempting to assign a nonexistent entity to be a pawn");
        }
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        if self.pawn_entity_store.contains(key) {
            self.pawn_entity_store.remove(key);
            let local_key = self.local_entity_store.get(key)
                .expect("expecting an entity record to exist if that entity is designated as a pawn")
                .local_key;

            self.queued_messages
                .push_back(ServerActorMessage::UnassignPawnEntity(
                    *key,
                    local_key,
                ));
        } else {
            panic!("attempting to unassign an entity as a pawn which is not assigned as a pawn in the first place")
        }
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.pawn_entity_store.contains(key);
    }

    // Components

    // called when the entity already exists in this connection
    pub fn add_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey, component_ref: &Ref<dyn Actor<T>>) {
        if !self.local_entity_store.contains_key(entity_key) {
            panic!("attempting to add component to entity that does not yet exist for this connection");
        }

        let local_component_key = self.actor_init(component_key, component_ref, LocalityStatus::Creating);

        let entity_record = self.local_entity_store.get(entity_key).unwrap();

        match entity_record.status {
            LocalityStatus::Creating => {
                // uncreated components will be created after entity is created
            }
            LocalityStatus::Created => {
                // send add component message
                self.queued_messages
                    .push_back(ServerActorMessage::AddComponent(
                        entity_record.local_key,
                        *component_key,
                        local_component_key,
                        component_ref.clone(),
                    ));
            }
            LocalityStatus::Deleting => {
                // deletion in progress, do nothing
            }
        }
    }

    // Ect..

    pub fn get_global_key_from_local(&self, local_key: LocalActorKey) -> Option<&ActorKey> {
        return self.local_to_global_key_map.get(&local_key);
    }

    pub fn get_global_entity_key_from_local(&self, local_key: LocalEntityKey) -> Option<&EntityKey> {
        return self.local_to_global_entity_key_map.get(&local_key);
    }

    pub fn collect_actor_updates(&mut self) {
        for (key, record) in self.actor_records.iter() {
            if record.status == LocalityStatus::Created
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

    pub fn write_actor_message<U: EventType>(
        &self,
        packet_writer: &mut ServerPacketWriter,
        manifest: &Manifest<U, T>,
        message: &ServerActorMessage<T>,
    ) -> bool {
        let mut actor_total_bytes = Vec::<u8>::new();

        //Write actor message type
        actor_total_bytes
                    .write_u8(message.as_type().to_u8())
                    .unwrap(); // write actor message type

        match message {
            ServerActorMessage::CreateActor(_, local_key, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor.borrow().write(&mut actor_payload_bytes);

                //Write actor "header"
                let type_id = actor.borrow().get_type_id();
                let naia_id = manifest.get_actor_naia_id(&type_id); // get naia id
                actor_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
            ServerActorMessage::DeleteActor(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::UpdateActor(_, local_key, state_mask, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor
                    .borrow()
                    .write_partial(&state_mask.borrow(), &mut actor_payload_bytes);

                //Write actor "header"
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                state_mask.borrow_mut().write(&mut actor_total_bytes); // write state mask
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
            ServerActorMessage::AssignPawn(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::UnassignPawn(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::UpdatePawn(_, local_key, _, actor) => {
                //write actor payload
                let mut actor_payload_bytes = Vec::<u8>::new();
                actor.borrow().write(&mut actor_payload_bytes);

                //Write actor "header"
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
                actor_total_bytes.append(&mut actor_payload_bytes); // write payload
            }
            ServerActorMessage::CreateEntity(_, local_entity_key, component_list_opt) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key

                // get list of components
                if let Some(component_list) = component_list_opt {

                    let components_num = component_list.len();
                    if components_num > 255 {
                        panic!("no entity should have so many components... fix this");
                    }
                    actor_total_bytes
                        .write_u8(components_num as u8)
                        .unwrap(); //write number of components

                    for (_, local_component_key, component_ref) in component_list {
                        //write component payload
                        let mut component_payload_bytes = Vec::<u8>::new();
                        component_ref.borrow().write(&mut component_payload_bytes);

                        //Write component "header"
                        let type_id = component_ref.borrow().get_type_id();
                        let naia_id = manifest.get_actor_naia_id(&type_id); // get naia id
                        actor_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                        actor_total_bytes
                            .write_u16::<BigEndian>(local_component_key.to_u16())
                            .unwrap(); //write local key
                        actor_total_bytes.append(&mut component_payload_bytes); // write payload
                    }
                } else {
                    actor_total_bytes
                        .write_u8(0)
                        .unwrap();
                }
            }
            ServerActorMessage::DeleteEntity(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::AssignPawnEntity(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::UnassignPawnEntity(_, local_key) => {
                actor_total_bytes
                    .write_u16::<BigEndian>(local_key.to_u16())
                    .unwrap(); //write local key
            }
            ServerActorMessage::AddComponent(local_entity_key, _, local_component_key, component) => {
                //write component payload
                let mut component_payload_bytes = Vec::<u8>::new();
                component.borrow().write(&mut component_payload_bytes);

                //Write component "header"
                actor_total_bytes
                    .write_u16::<BigEndian>(local_entity_key.to_u16())
                    .unwrap(); //write local entity key
                let type_id = component.borrow().get_type_id();
                let naia_id = manifest.get_actor_naia_id(&type_id); // get naia id
                actor_total_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                actor_total_bytes
                    .write_u16::<BigEndian>(local_component_key.to_u16())
                    .unwrap(); //write local component key
                actor_total_bytes.append(&mut component_payload_bytes); // write payload
            }
        }

        let mut hypothetical_next_payload_size =
            packet_writer.bytes_number() + actor_total_bytes.len();
        if packet_writer.actor_message_count == 0 {
            hypothetical_next_payload_size += 2;
        }
        if hypothetical_next_payload_size < MTU_SIZE {
            if packet_writer.actor_message_count == 255 {
                return false;
            }
            packet_writer.actor_message_count = packet_writer.actor_message_count.wrapping_add(1);
            packet_writer
                .actor_working_bytes
                .append(&mut actor_total_bytes);
            return true;
        } else {
            return false;
        }
    }

    // Private methods

    fn actor_init(&mut self, key: &ActorKey, actor: &Ref<dyn Actor<T>>, status: LocalityStatus) -> LocalActorKey {
        if !self.local_actor_store.contains_key(*key) {
            self.local_actor_store.insert(*key, actor.clone());
            let local_key: LocalActorKey = self.actor_key_generator.generate();
            self.local_to_global_key_map.insert(local_key, *key);
            let state_mask_size = actor.borrow().get_state_mask_size();
            let actor_record = ActorRecord::new(local_key, state_mask_size, status);
            self.mut_handler.borrow_mut().register_mask(
                &self.address,
                &key,
                actor_record.get_state_mask(),
            );
            self.actor_records.insert(*key, actor_record);
            return local_key;
        } else {
            // Should panic, as this is not dependent on any unreliable transport factor
            panic!("attempted to add actor twice..");
        }
    }

    fn actor_cleanup(&mut self, global_actor_key: &ActorKey) {
        if let Some(actor_record) = self.actor_records.remove(*global_actor_key) {
            // actually delete the actor from local records
            let local_actor_key = actor_record.local_key;
            self.mut_handler
                .borrow_mut()
                .deregister_mask(&self.address, global_actor_key);
            self.local_actor_store.remove(*global_actor_key);
            self.local_to_global_key_map.remove(&local_actor_key);
            self.actor_key_generator.recycle_key(&local_actor_key);
            self.pawn_store.remove(&global_actor_key);
        } else {
            // likely due to duplicate delivered deletion messages
            warn!("attempting to clean up actor from connection inside which it is not present");
        }
    }

    fn pop_create_actor_state_mask(&mut self, global_key: &ActorKey) {
        if let Some(record) = self.actor_records.get(*global_key) {
            self.last_popped_state_mask = Some(record.get_state_mask().borrow().clone());
        }
        self.mut_handler
            .borrow_mut()
            .clear_state(&self.address, global_key);
    }

    fn unpop_create_actor_state_mask(&mut self, global_key: &ActorKey) {
        if let Some(last_popped_state_mask) = &self.last_popped_state_mask {
            self.mut_handler.borrow_mut().set_state(
                &self.address,
                global_key,
                &last_popped_state_mask,
            );
        }
    }

    fn pop_update_actor_state_mask(&mut self,
                                   is_pawn: bool,
                                   packet_index: u16,
                                   global_key: &ActorKey,
                                   local_key: &LocalActorKey,
                                   state_mask: &Ref<StateMask>,
                                   actor: &Ref<dyn Actor<T>>) -> ServerActorMessage<T> {
        let locked_state_mask =
            self.process_actor_update(packet_index, global_key, state_mask);
        // return new Update message to be written
        if is_pawn {
            return ServerActorMessage::UpdatePawn(
                *global_key,
                *local_key,
                locked_state_mask,
                actor.clone(),
            );
        } else {
            return ServerActorMessage::UpdateActor(
                *global_key,
                *local_key,
                locked_state_mask,
                actor.clone(),
            );
        }
    }

    fn unpop_update_actor_state_mask(&mut self,
                                   is_pawn: bool,
                                   packet_index: u16,
                                   global_key: &ActorKey,
                                   local_key: &LocalActorKey,
                                   actor: &Ref<dyn Actor<T>>) -> ServerActorMessage<T> {
        let original_state_mask = self.undo_actor_update(&packet_index, &global_key);
        if is_pawn {
            return ServerActorMessage::UpdatePawn(
                *global_key,
                *local_key,
                original_state_mask,
                actor.clone(),
            );
        } else {
            return ServerActorMessage::UpdateActor(
                *global_key,
                *local_key,
                original_state_mask,
                actor.clone(),
            );
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
        self.last_popped_state_mask = Some(state_mask.borrow().clone());
        self.mut_handler
            .borrow_mut()
            .clear_state(&self.address, global_key);

        locked_state_mask
    }

    fn undo_actor_update(&mut self, packet_index: &u16, global_key: &ActorKey) -> Ref<StateMask> {
        if let Some(sent_updates_map) = self.sent_updates.get_mut(packet_index) {
            sent_updates_map.remove(global_key);
            if sent_updates_map.len() == 0 {
                self.sent_updates.remove(&packet_index);
            }
        }

        self.last_update_packet_index = self.last_last_update_packet_index;
        if let Some(last_popped_state_mask) = &self.last_popped_state_mask {
            self.mut_handler.borrow_mut().set_state(
                &self.address,
                global_key,
                &last_popped_state_mask,
            );
        }

        self.actor_records
            .get(*global_key)
            .expect("uh oh, we don't have enough info to unpop the message")
            .get_state_mask()
            .clone()
    }
}

impl<T: ActorType> ActorNotifiable for ServerActorManager<T> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        let mut deleted_actors: Vec<ActorKey> = Vec::new();

        if let Some(delivered_messages_list) = self.sent_messages.remove(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {
                match delivered_message {
                    ServerActorMessage::CreateActor(global_key, _, _) => {
                        let actor_record = self.actor_records.get_mut(global_key)
                            .expect("created actor does not have an actor_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_actor_deletions.remove(&global_key) {
                            actor_delete(&mut self.queued_messages, actor_record, &global_key);
                        } else {
                            // we do not need to delete just yet
                            actor_record.status = LocalityStatus::Created;
                        }
                    }
                    ServerActorMessage::DeleteActor(global_actor_key, _) => {
                        deleted_actors.push(global_actor_key);
                    }
                    ServerActorMessage::UpdateActor(_, _, _, _)
                    | ServerActorMessage::UpdatePawn(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                    ServerActorMessage::AssignPawn(_, _) => {}
                    ServerActorMessage::UnassignPawn(_, _) => {}
                    ServerActorMessage::CreateEntity(global_entity_key, _, component_list_opt) => {
                        let entity_record = self.local_entity_store.get_mut(&global_entity_key)
                            .expect("created entity does not have a entity_record ... initialization error?");

                        // do we need to delete this now?
                        if self.delayed_entity_deletions.remove(&global_entity_key) {
                            entity_delete(&mut self.queued_messages, entity_record, &global_entity_key);
                        } else {
                            // set to status of created
                            entity_record.status = LocalityStatus::Created;

                            // set status of components to created
                            if let Some(mut component_list) = component_list_opt {
                                while let Some((global_component_key, _, _)) = component_list.pop() {
                                    let component_record = self.actor_records.get_mut(global_component_key)
                                        .expect("component not created correctly?");
                                    component_record.status = LocalityStatus::Created;
                                }
                            }

                            // for any components on this entity that have not yet been created
                            // initiate that now
                            let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                            for component_key in component_set {
                                let component_record = self.actor_records.get(*component_key)
                                    .expect("component not created correctly?");
                                // check if component has been successfully created
                                // (perhaps through the previous entity_create operation)
                                if component_record.status == LocalityStatus::Creating {
                                    let component_ref = self.local_actor_store.get(*component_key)
                                        .expect("component not created correctly?");
                                    self.queued_messages
                                        .push_back(ServerActorMessage::AddComponent(
                                            entity_record.local_key,
                                            *component_key,
                                            component_record.local_key,
                                            component_ref.clone(),
                                        ));
                                }
                            }
                        }
                    }
                    ServerActorMessage::DeleteEntity(global_key, local_key) => {
                        let entity_record = self.local_entity_store.remove(&global_key).expect("deletion of nonexistent entity!");

                        // actually delete the entity from local records
                        self.local_to_global_entity_key_map.remove(&local_key);
                        self.entity_key_generator.recycle_key(&local_key);
                        self.pawn_entity_store.remove(&global_key);

                        // delete all associated component actors
                        let component_set: &HashSet<ComponentKey> = &entity_record.components_ref.borrow();
                        for component_key in component_set {
                            deleted_actors.push(*component_key);
                        }
                    }
                    ServerActorMessage::AssignPawnEntity(_, _) => {}
                    ServerActorMessage::UnassignPawnEntity(_, _) => {}
                    ServerActorMessage::AddComponent(_, global_component_key, _, _) => {
                        let component_record = self.actor_records.get_mut(global_component_key)
                            .expect("added component does not have a record .. initiation problem?");
                        // do we need to delete this now?
                        if self.delayed_actor_deletions.remove(&global_component_key) {
                            actor_delete(&mut self.queued_messages, component_record, &global_component_key);
                        } else {
                            // we do not need to delete just yet
                            component_record.status = LocalityStatus::Created;
                        }
                    }
                }
            }
        }

        for deleted_actor_key in deleted_actors {
            self.actor_cleanup(&deleted_actor_key);
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
                    | ServerActorMessage::CreateEntity(_, _, _)
                    | ServerActorMessage::DeleteEntity(_, _)
                    | ServerActorMessage::AssignPawnEntity(_, _)
                    | ServerActorMessage::UnassignPawnEntity(_, _)
                    | ServerActorMessage::AddComponent(_, _, _, _) => {
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

fn actor_delete<T: ActorType>(queued_messages: &mut VecDeque<ServerActorMessage<T>>,
                              actor_record: &mut ActorRecord,
                              actor_key: &ActorKey) {
    actor_record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ServerActorMessage::DeleteActor(
            *actor_key,
            actor_record.local_key,
        ));
}

fn entity_delete<T: ActorType>(queued_messages: &mut VecDeque<ServerActorMessage<T>>,
                              entity_record: &mut EntityRecord,
                              entity_key: &EntityKey) {
    entity_record.status = LocalityStatus::Deleting;

    queued_messages.push_back(ServerActorMessage::DeleteEntity(
        *entity_key,
        entity_record.local_key,
    ));
}