use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
};

use log::warn;

use naia_shared::{DiffMask, EntityActionType, LocalComponentKey, LocalEntity as OldEntity, Manifest, NaiaKey, PacketReader, ProtocolType, Replicate, WorldRefType, EntityType, WorldMutType};

use super::{
    command_receiver::CommandReceiver, entity_action::EntityAction, entity_record::EntityRecord,
};

#[derive(Debug)]
pub struct EntityManager<P: ProtocolType, K: EntityType> {
    entities: HashMap<K, EntityRecord>,
    local_to_world_entity: HashMap<OldEntity, K>,
    component_to_entity_map: HashMap<LocalComponentKey, K>,
    queued_incoming_messages: VecDeque<EntityAction<P, K>>,
}

impl<P: ProtocolType, K: EntityType> EntityManager<P, K> {
    pub fn new() -> Self {
        EntityManager {
            local_to_world_entity: HashMap::new(),
            entities: HashMap::new(),
            component_to_entity_map: HashMap::new(),
//            prediction_components: HashMap::new(),
//            component_entity_map: HashMap::new(),
            queued_incoming_messages: VecDeque::new(),
        }
    }

    pub fn process_data<W: WorldMutType<P, K>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        command_receiver: &mut CommandReceiver<P, K>,
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
                    let local_entity = OldEntity::from_u16(reader.read_u16());
                    let components_num = reader.read_u8();
                    if self.local_to_world_entity.contains_key(&local_entity) {
                        // its possible we received a very late duplicate message
                        warn!("attempted to insert duplicate entity");
                        // continue reading, just don't do anything with the data
                        for _ in 0..components_num {
                            let naia_id: u16 = reader.read_u16();
                            let _component_key = reader.read_u16();
                            manifest.create_replica(naia_id, reader);
                        }
                    } else {
                        // set up entity
                        let world_entity = world.spawn_entity();
                        self.local_to_world_entity.insert(local_entity, world_entity);
                        self.entities.insert(world_entity, EntityRecord::new());
                        let mut entity_record = self.entities.get_mut(&world_entity).unwrap();

                        let mut component_list: Vec<P> = Vec::new();
                        for _ in 0..components_num {
                            // Component Creation //
                            let naia_id: u16 = reader.read_u16();
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_component = manifest.create_replica(naia_id, reader);
                            if self.component_to_entity_map.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                let new_component_type_id = new_component.get_type_id();
                                self.component_to_entity_map.insert(component_key, world_entity);
                                world.insert_component(&world_entity, new_component.clone());
                                component_list.push(new_component);
                                entity_record
                                    .insert_component(&component_key, &new_component_type_id);
                            }
                            ////////////////////////
                        }

                        self.queued_incoming_messages
                            .push_back(EntityAction::SpawnEntity(world_entity, component_list));
                        continue;
                    }
                }
                EntityActionType::DespawnEntity => {
                    // Entity Deletion
                    let local_entity = OldEntity::from_u16(reader.read_u16());
                    if let Some(world_entity) = self.local_to_world_entity.remove(&local_entity) {
                        if let Some(entity_record) = self.entities.remove(&world_entity) {
                            if entity_record.is_prediction {
                                command_receiver.prediction_cleanup(&world_entity);
                            }

                            for component_key in entity_record.get_component_keys() {
                                // delete all components //
                                self.component_delete_cleanup(&world_entity, &component_key);

                                self.component_to_entity_map.remove(&component_key);
                                ////////////////////////////
                            }

                            self.queued_incoming_messages
                                .push_back(EntityAction::DespawnEntity(world_entity));
                            continue;
                        }
                    }
                    warn!(
                        "received message attempting to delete nonexistent entity"
                    );
                }
                EntityActionType::OwnEntity => {
                    // Assign Prediction Entity
                    let local_entity = OldEntity::from_u16(reader.read_u16());
                    if let Some(world_entity) = self.local_to_world_entity.remove(&local_entity) {
                        if let Some(entity_record) = self.entities.get_mut(&world_entity) {
                            entity_record.is_prediction = true;

                            // create copies of components //
                            for component_key in entity_record.get_component_keys() {
                                if let Some(protocol) = self.component_to_entity_map.get(&component_key) {
                                    self.prediction_components
                                        .insert(component_key, protocol.copy());
                                }
                            }
                            /////////////////////////////////

                            command_receiver.prediction_init(&local_entity);

                            self.queued_incoming_messages
                                .push_back(EntityAction::OwnEntity(local_entity));
                        }
                    }
                }
                EntityActionType::DisownEntity => {
                    // Unassign Prediction Entity
                    let entity_key = OldEntity::from_u16(reader.read_u16());
                    if let Some(entity_record) = self.entities.get_mut(&entity_key) {
                        if entity_record.is_prediction {
                            entity_record.is_prediction = false;

                            // remove prediction components //
                            for component_key in entity_record.get_component_keys() {
                                self.prediction_components.remove(&component_key);
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
                    let entity_key = OldEntity::from_u16(reader.read_u16());
                    let naia_id: u16 = reader.read_u16();
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    let new_component = manifest.create_replica(naia_id, reader);
                    if self.component_to_entity_map.contains_key(&component_key) {
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
                            self.component_to_entity_map.insert(component_key, new_component.clone());

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

                    if let Some(component_ref) = self.component_to_entity_map.get_mut(&component_key) {
                        let diff_mask: DiffMask = DiffMask::read(reader);

                        component_ref.read_partial(&diff_mask, reader, packet_index);

                        let entity_key = self
                            .component_entity_map
                            .get(&component_key)
                            .expect("component not initialized correctly");

                        // check if Entity is a Prediction
                        if self
                            .entities
                            .get(entity_key)
                            .expect("component has no associated entity?")
                            .is_prediction
                        {
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

                    if !self.component_entity_map.contains_key(&component_key) {
                        // This could happen due to a duplicated unreliable message
                        // (i.e. server re-sends "remove component" message because it believes it
                        // hasn't been delivered and then it does get
                        // delivered after, but then a duplicate message gets delivered too..)
                        warn!(
                            "attempting to remove a non-existent component: {}",
                            component_key.to_u16()
                        );
                    } else {
                        let entity_key = self.component_entity_map.remove(&component_key).unwrap();

                        // Get entity record, remove component
                        self.entities
                            .get_mut(&entity_key)
                            .expect("entity not instantiated properly?")
                            .remove_component(&component_key);
                        self.component_delete_cleanup(&entity_key, &component_key);
                    }
                }
                EntityActionType::Unknown => {
                    panic!("received unknown type of entity action");
                }
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<EntityAction<P, K>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn get_component_by_type<R: Replicate<P>>(&self, key: &K) -> Option<&P> {
        if let Some(entity_record) = self.entities.get(key) {
            if let Some(component_key) = entity_record.get_key_from_type(&TypeId::of::<R>()) {
                return self.component_to_entity_map.get(component_key);
            }
        }
        return None;
    }

    pub fn get_prediction_component_by_type<R: Replicate<P>>(
        &self,
        key: &K,
    ) -> Option<&P> {
        if let Some(entity_component_record) = self.entities.get(key) {
            if let Some(component_key) =
                entity_component_record.get_key_from_type(&TypeId::of::<R>())
            {
                return self.prediction_components.get(component_key);
            }
        }
        return None;
    }

    pub fn has_entity(&self, key: &K) -> bool {
        return self.entities.contains_key(key);
    }

    pub fn entity_is_prediction(&self, key: &K) -> bool {
        if let Some(entity_record) = self.entities.get(key) {
            return entity_record.is_prediction;
        }
        return false;
    }

    pub fn prediction_reset_entity(&mut self, key: &K) {
        if let Some(entity_record) = self.entities.get(key) {
            for component_key in entity_record.get_component_keys() {
                if let Some(component_ref) = self.component_to_entity_map.get(&component_key) {
                    if let Some(prediction_component_ref) =
                        self.prediction_components.get_mut(&component_key)
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
        entity_key: &K,
        component_key: &LocalComponentKey,
    ) {
        self.prediction_components.remove(&component_key);

        if let Some(component) = self.component_to_entity_map.remove(&component_key) {
            self.queued_incoming_messages
                .push_back(EntityAction::RemoveComponent(*entity_key, component));
        }
    }
}
