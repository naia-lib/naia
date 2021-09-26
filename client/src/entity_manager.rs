use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
};

use log::warn;

use naia_shared::{
    DiffMask, EntityActionType, LocalComponentKey, LocalEntityKey, Manifest, NaiaKey, PacketReader,
    ProtocolType, Replicate,
};

use super::{
    command_receiver::CommandReceiver, entity_action::EntityAction, entity_record::EntityRecord,
};

#[derive(Debug)]
pub struct EntityManager<P: ProtocolType> {
    entities: HashMap<LocalEntityKey, EntityRecord>,
    component_store: HashMap<LocalComponentKey, P>,
    prediction_component_store: HashMap<LocalComponentKey, P>,
    component_entity_map: HashMap<LocalComponentKey, LocalEntityKey>,
    queued_incoming_messages: VecDeque<EntityAction<P>>,
}

impl<P: ProtocolType> EntityManager<P> {
    pub fn new() -> Self {
        EntityManager {
            entities: HashMap::new(),
            component_store: HashMap::new(),
            prediction_component_store: HashMap::new(),
            component_entity_map: HashMap::new(),
            queued_incoming_messages: VecDeque::new(),
        }
    }

    pub fn process_data(
        &mut self,
        manifest: &Manifest<P>,
        command_receiver: &mut CommandReceiver<P>,
        packet_tick: u16,
        packet_index: u16,
        reader: &mut PacketReader,
    ) {
        let entity_action_count = reader.read_u8();

        for _ in 0..entity_action_count {
            let message_type = EntityActionType::from_u8(reader.read_u8());

            match message_type {
                EntityActionType::SpawnEntity => {
                    // Entity Creation
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let components_num = reader.read_u8();
                    if self.entities.contains_key(&entity_key) {
                        // its possible we received a very late duplicate message
                        warn!("attempted to insert duplicate entity");
                        // continue reading, just don't do anything with the data
                        for _ in 0..components_num {
                            let naia_id: u16 = reader.read_u16();
                            let _component_key = reader.read_u16();
                            manifest.create_replica(naia_id, reader);
                        }
                    } else {
                        let mut component_list: Vec<P> = Vec::new();
                        let mut entity_record = EntityRecord::new();

                        for _ in 0..components_num {
                            // Component Creation //
                            let naia_id: u16 = reader.read_u16();
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_component = manifest.create_replica(naia_id, reader);
                            if self.component_store.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                let new_component_type_id = new_component.get_type_id();
                                self.component_store
                                    .insert(component_key, new_component.clone());
                                self.component_entity_map.insert(component_key, entity_key);
                                component_list.push(new_component);
                                entity_record
                                    .insert_component(&component_key, &new_component_type_id);
                            }
                            ////////////////////////
                        }

                        self.entities.insert(entity_key, entity_record);

                        self.queued_incoming_messages
                            .push_back(EntityAction::SpawnEntity(entity_key, component_list));
                    }
                }
                EntityActionType::DespawnEntity => {
                    // Entity Deletion
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());

                    if let Some(entity_record) = self.entities.remove(&entity_key) {
                        if entity_record.is_prediction {
                            command_receiver.prediction_cleanup(&entity_key);
                        }

                        for component_key in entity_record.get_component_keys() {
                            // delete all components //
                            self.component_delete_cleanup(&entity_key, &component_key);

                            self.component_entity_map.remove(&component_key);
                            ////////////////////////////
                        }

                        self.queued_incoming_messages
                            .push_back(EntityAction::DespawnEntity(entity_key));
                    } else {
                        // its possible we received a very late duplicate message
                        warn!(
                            "received message attempting to delete nonexistent entity: {}",
                            entity_key.to_u16()
                        );
                    }
                }
                EntityActionType::OwnEntity => {
                    // Assign Prediction Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if let Some(entity_record) = self.entities.get_mut(&entity_key) {
                        entity_record.is_prediction = true;

                        // create copies of components //
                        for component_key in entity_record.get_component_keys() {
                            if let Some(protocol) = self.component_store.get(&component_key) {
                                self.prediction_component_store
                                    .insert(component_key, protocol.copy());
                            }
                        }
                        /////////////////////////////////

                        command_receiver.prediction_init(&entity_key);

                        self.queued_incoming_messages
                            .push_back(EntityAction::OwnEntity(entity_key));
                    }
                }
                EntityActionType::DisownEntity => {
                    // Unassign Prediction Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    if let Some(entity_record) = self.entities.get_mut(&entity_key) {
                        if entity_record.is_prediction {
                            entity_record.is_prediction = false;

                            // remove prediction components //
                            for component_key in entity_record.get_component_keys() {
                                self.prediction_component_store.remove(&component_key);
                            }
                            ////////////////////////////

                            command_receiver.prediction_cleanup(&entity_key);

                            self.queued_incoming_messages
                                .push_back(EntityAction::DisownEntity(entity_key));
                        }
                    }
                }
                EntityActionType::InsertComponent => {
                    //TODO: handle adding Component to a Prediction...

                    // Add Component to Entity
                    let entity_key = LocalEntityKey::from_u16(reader.read_u16());
                    let naia_id: u16 = reader.read_u16();
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    let new_component = manifest.create_replica(naia_id, reader);
                    if self.component_store.contains_key(&component_key) {
                        // its possible we received a very late duplicate message
                        warn!(
                            "attempting to add duplicate local component key: {}, into entity: {}",
                            component_key.to_u16(),
                            entity_key.to_u16()
                        );
                    } else {
                        if !self.entities.contains_key(&entity_key) {
                            // its possible we received a very late duplicate message
                            warn!(
                                "attempting to add a component: {}, to nonexistent entity: {}",
                                component_key.to_u16(),
                                entity_key.to_u16()
                            );
                        } else {
                            self.component_store
                                .insert(component_key, new_component.clone());

                            self.component_entity_map.insert(component_key, entity_key);
                            let entity_record = self.entities.get_mut(&entity_key).unwrap();

                            entity_record
                                .insert_component(&component_key, &new_component.get_type_id());

                            self.queued_incoming_messages
                                .push_back(EntityAction::InsertComponent(
                                    entity_key,
                                    new_component,
                                ));
                        }
                    }
                }
                EntityActionType::UpdateComponent => {
                    // Component Update
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    if let Some(component_ref) = self.component_store.get_mut(&component_key) {
                        let diff_mask: DiffMask = DiffMask::read(reader);

                        component_ref.read_partial(&diff_mask, reader, packet_index);

                        let entity_key = self
                            .component_entity_map
                            .get(&component_key)
                            .expect("component not initialized correctly");

                        // check if Entity is a Prediction
                        if self.entities
                            .get(entity_key)
                            .expect("component has no associated entity?")
                            .is_prediction {

                            // replay commands
                            command_receiver.replay_commands(packet_tick, &entity_key);

                            // remove command history until the tick that has already been
                            // checked
                            command_receiver.remove_history_until(packet_tick, &entity_key);
                        }

                        self.queued_incoming_messages
                            .push_back(EntityAction::UpdateComponent(
                                *entity_key,
                                component_ref.clone(),
                            ));
                    }
                }
                EntityActionType::RemoveComponent => {
                    // Component Removal
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    let entity_key = self
                        .component_entity_map
                        .remove(&component_key)
                        .expect("deleting nonexistant/non-initialized component");

                    // Get entity record, remove component
                    self
                        .entities
                        .get_mut(&entity_key)
                        .expect("entity not instantiated properly?")
                        .remove_component(&component_key);
                    self.component_delete_cleanup(&entity_key, &component_key);
                }
                EntityActionType::Unknown => {
                    panic!("received unknown type of entity action");
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<EntityAction<P>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn get_component_by_type<R: Replicate<P>>(&self, key: &LocalEntityKey) -> Option<&P> {
        if let Some(entity_record) = self.entities.get(key) {
            if let Some(component_key) = entity_record.get_key_from_type(&TypeId::of::<R>()) {
                return self.component_store.get(component_key);
            }
        }
        return None;
    }

    pub fn get_prediction_component_by_type<R: Replicate<P>>(
        &self,
        key: &LocalEntityKey,
    ) -> Option<&P> {
        if let Some(entity_component_record) = self.entities.get(key) {
            if let Some(component_key) =
                entity_component_record.get_key_from_type(&TypeId::of::<R>())
            {
                return self.prediction_component_store.get(component_key);
            }
        }
        return None;
    }

    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        return self.entities.contains_key(key);
    }

    pub fn entity_keys(&self) -> Vec<LocalEntityKey> {
        let mut output = Vec::new();

        for (key, _) in self.entities.iter() {
            output.push(*key);
        }

        return output;
    }

    pub fn entity_is_prediction(&self, key: &LocalEntityKey) -> bool {
        if let Some(entity_record) = self.entities.get(key) {
            return entity_record.is_prediction;
        }
        return false;
    }

    pub fn prediction_reset_entity(&mut self, key: &LocalEntityKey) {
        if let Some(entity_record) = self.entities.get(key) {
            for component_key in entity_record.get_component_keys() {
                if let Some(component_ref) = self.component_store.get(&component_key) {
                    if let Some(prediction_component_ref) =
                        self.prediction_component_store.get_mut(&component_key)
                    {
                        prediction_component_ref.mirror(component_ref);
                    }
                }
            }
        }

        self.queued_incoming_messages
            .push_back(EntityAction::RewindEntity(*key));
    }

    // internal

    fn component_delete_cleanup(
        &mut self,
        entity_key: &LocalEntityKey,
        component_key: &LocalComponentKey,
    ) {
        self.prediction_component_store.remove(&component_key);

        if let Some(component) = self.component_store.remove(&component_key) {
            self.queued_incoming_messages
                .push_back(EntityAction::RemoveComponent(*entity_key, component));
        }
    }
}
