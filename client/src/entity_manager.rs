use std::{
    collections::{HashMap, VecDeque},
    ops::Deref,
};

use log::warn;

use naia_shared::{
    ComponentDynMut, DiffMask, EntityActionType, EntityType, LocalComponentKey, LocalEntity,
    Manifest, NaiaKey, PacketReader, ProtocolKindType, ProtocolType, WorldMutType,
};

use super::{
    command_receiver::CommandReceiver, entity_action::EntityAction, entity_record::EntityRecord,
    owned_entity::OwnedEntity,
};

pub struct EntityManager<P: ProtocolType, E: EntityType> {
    entity_records: HashMap<E, EntityRecord<E, P::Kind>>,
    local_to_world_entity: HashMap<LocalEntity, E>,
    component_to_entity_map: HashMap<LocalComponentKey, E>,
    predicted_to_confirmed_entity: HashMap<E, E>,
    queued_incoming_messages: VecDeque<EntityAction<P, E>>,
}

impl<P: ProtocolType, E: EntityType> EntityManager<P, E> {
    pub fn new() -> Self {
        EntityManager {
            local_to_world_entity: HashMap::new(),
            entity_records: HashMap::new(),
            component_to_entity_map: HashMap::new(),
            predicted_to_confirmed_entity: HashMap::new(),
            queued_incoming_messages: VecDeque::new(),
        }
    }

    pub fn process_data<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        command_receiver: &mut CommandReceiver<P, E>,
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
                    let local_entity = LocalEntity::from_u16(reader.read_u16());
                    let components_num = reader.read_u8();
                    if self.local_to_world_entity.contains_key(&local_entity) {
                        // its possible we received a very late duplicate message
                        warn!("attempted to insert duplicate entity");
                        // continue reading, just don't do anything with the data
                        for _ in 0..components_num {
                            let component_kind = P::Kind::from_u16(reader.read_u16());
                            let _component_key = reader.read_u16();
                            manifest.create_replica(component_kind, reader, packet_index);
                        }
                    } else {
                        // set up entity
                        let world_entity = world.spawn_entity();
                        self.local_to_world_entity
                            .insert(local_entity, world_entity);
                        self.entity_records
                            .insert(world_entity, EntityRecord::new(&local_entity));
                        let entity_record = self.entity_records.get_mut(&world_entity).unwrap();

                        let mut component_list: Vec<P::Kind> = Vec::new();
                        for _ in 0..components_num {
                            // Component Creation //
                            let component_kind = P::Kind::from_u16(reader.read_u16());
                            let component_key = LocalComponentKey::from_u16(reader.read_u16());

                            let new_component =
                                manifest.create_replica(component_kind, reader, packet_index);
                            if self.component_to_entity_map.contains_key(&component_key) {
                                panic!("attempted to insert duplicate component");
                            } else {
                                {
                                    let new_component_kind = new_component.dyn_ref().get_kind();
                                    entity_record
                                        .insert_component(&component_key, &new_component_kind);
                                    component_list.push(new_component_kind);
                                }

                                self.component_to_entity_map
                                    .insert(component_key, world_entity);
                                new_component.extract_and_insert(&world_entity, world);
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
                    let local_entity = LocalEntity::from_u16(reader.read_u16());
                    if let Some(world_entity) = self.local_to_world_entity.remove(&local_entity) {
                        if let Some(entity_record) = self.entity_records.remove(&world_entity) {
                            if entity_record.is_owned() {
                                let prediction_entity = entity_record.get_prediction().unwrap();
                                self.predicted_to_confirmed_entity
                                    .remove(&prediction_entity);
                                command_receiver.prediction_cleanup(&world_entity);
                            }

                            // Generate event for each component, handing references off just in
                            // case
                            for component_kind in world.get_component_kinds(&world_entity) {
                                if let Some(component) =
                                    world.remove_component_of_kind(&world_entity, &component_kind)
                                {
                                    self.queued_incoming_messages.push_back(
                                        EntityAction::RemoveComponent(world_entity, component),
                                    );
                                }
                            }

                            for component_key in entity_record.get_component_keys() {
                                self.component_to_entity_map.remove(&component_key);
                            }

                            world.despawn_entity(&world_entity);

                            self.queued_incoming_messages
                                .push_back(EntityAction::DespawnEntity(world_entity));
                            continue;
                        }
                    }
                    warn!("received message attempting to delete nonexistent entity");
                }
                EntityActionType::OwnEntity => {
                    // Assign Prediction Entity
                    let local_entity = LocalEntity::from_u16(reader.read_u16());
                    if let Some(world_entity) = self.local_to_world_entity.remove(&local_entity) {
                        if let Some(entity_record) = self.entity_records.get_mut(&world_entity) {
                            let prediction_entity = world.spawn_entity();

                            entity_record.set_prediction(&prediction_entity);
                            self.predicted_to_confirmed_entity
                                .insert(prediction_entity, world_entity);

                            // create copies of components //
                            for component_kind in world.get_component_kinds(&world_entity) {
                                let mut component_copy_opt: Option<P> = None;
                                if let Some(component) =
                                    world.get_component_mut_of_kind(&world_entity, &component_kind)
                                {
                                    component_copy_opt =
                                        Some(component.deref().deref().protocol_copy());
                                }
                                if let Some(component_copy) = component_copy_opt {
                                    component_copy.extract_and_insert(&prediction_entity, world);
                                }
                            }
                            /////////////////////////////////

                            command_receiver.prediction_init(&world_entity);

                            self.queued_incoming_messages
                                .push_back(EntityAction::OwnEntity(OwnedEntity::new(
                                    &world_entity,
                                    &prediction_entity,
                                )));
                        }
                    }
                }
                EntityActionType::DisownEntity => {
                    // Unassign Prediction Entity
                    let local_entity = LocalEntity::from_u16(reader.read_u16());
                    if let Some(world_entity) = self.local_to_world_entity.get(&local_entity) {
                        if let Some(entity_record) = self.entity_records.get_mut(&world_entity) {
                            if entity_record.is_owned() {
                                let prediction_entity = entity_record.disown().unwrap();
                                self.predicted_to_confirmed_entity
                                    .remove(&prediction_entity);

                                world.despawn_entity(&prediction_entity);

                                command_receiver.prediction_cleanup(&world_entity);

                                self.queued_incoming_messages.push_back(
                                    EntityAction::DisownEntity(OwnedEntity::new(
                                        world_entity,
                                        &prediction_entity,
                                    )),
                                );
                            }
                        }
                    }
                }
                EntityActionType::InsertComponent => {
                    // Add Component to Entity
                    let local_entity = LocalEntity::from_u16(reader.read_u16());
                    let component_kind = P::Kind::from_u16(reader.read_u16());
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    let new_component =
                        manifest.create_replica(component_kind, reader, packet_index);
                    if self.component_to_entity_map.contains_key(&component_key) {
                        // its possible we received a very late duplicate message
                        warn!(
                            "attempting to add duplicate local component key: {}, into entity: {}",
                            component_key.to_u16(),
                            local_entity.to_u16()
                        );
                    } else {
                        if !self.local_to_world_entity.contains_key(&local_entity) {
                            // its possible we received a very late duplicate message
                            warn!(
                                "attempting to add a component: {}, to nonexistent entity: {}",
                                component_key.to_u16(),
                                local_entity.to_u16()
                            );
                        } else {
                            let world_entity =
                                self.local_to_world_entity.get(&local_entity).unwrap();
                            self.component_to_entity_map
                                .insert(component_key, *world_entity);

                            let entity_record = self.entity_records.get_mut(&world_entity).unwrap();

                            entity_record.insert_component(&component_key, &component_kind);

                            new_component.extract_and_insert(world_entity, world);

                            //TODO: handle inserting Component into an Entity that has a
                            // Prediction... !!!

                            self.queued_incoming_messages
                                .push_back(EntityAction::InsertComponent(
                                    *world_entity,
                                    component_kind,
                                ));
                        }
                    }
                }
                EntityActionType::UpdateComponent => {
                    // Component Update
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    if let Some(world_entity) = self.component_to_entity_map.get_mut(&component_key)
                    {
                        if let Some(entity_record) = self.entity_records.get(world_entity) {
                            let component_kind =
                                entity_record.get_kind_from_key(&component_key).unwrap();
                            if let Some(mut component_protocol) =
                                world.get_component_mut_of_kind(world_entity, component_kind)
                            {
                                // read incoming delta
                                let diff_mask: DiffMask = DiffMask::read(reader);
                                component_protocol.dyn_mut().read_partial(
                                    &diff_mask,
                                    reader,
                                    packet_index,
                                );

                                // check if Entity is Owned
                                if entity_record.is_owned() {
                                    // replay commands
                                    command_receiver.replay_commands(packet_tick, &world_entity);

                                    // remove command history until the tick that has already been
                                    // checked
                                    command_receiver
                                        .remove_history_until(packet_tick, &world_entity);
                                }

                                self.queued_incoming_messages.push_back(
                                    EntityAction::UpdateComponent(*world_entity, *component_kind),
                                );
                            }
                        }
                    }
                }
                EntityActionType::RemoveComponent => {
                    // Component Removal
                    let component_key = LocalComponentKey::from_u16(reader.read_u16());

                    if !self.component_to_entity_map.contains_key(&component_key) {
                        // This could happen due to a duplicated unreliable message
                        // (i.e. server re-sends "remove component" message because it believes it
                        // hasn't been delivered and then it does get
                        // delivered after, but then a duplicate message gets delivered too..)
                        warn!(
                            "attempting to remove a non-existent component: {}",
                            component_key.to_u16()
                        );
                    } else {
                        let world_entity =
                            self.component_to_entity_map.remove(&component_key).unwrap();

                        // Get entity record, remove component
                        let component_kind = self
                            .entity_records
                            .get_mut(&world_entity)
                            .expect("entity not instantiated properly? no such entity")
                            .remove_component(&component_key)
                            .expect("entity not instantiated properly? no type");

                        // Get component for last change
                        let component = world
                            .remove_component_of_kind(&world_entity, &component_kind)
                            .expect("Component already removed?");

                        // Generate event
                        self.queued_incoming_messages
                            .push_back(EntityAction::RemoveComponent(world_entity, component));
                    }
                }
                EntityActionType::Unknown => {
                    panic!("received unknown type of entity action");
                }
            }
        }
    }

    pub fn world_to_local_entity(&self, world_entity: &E) -> Option<LocalEntity> {
        if let Some(entity_record) = self.entity_records.get(world_entity) {
            return Some(entity_record.local_entity());
        }
        return None;
    }

    pub fn get_predicted_entity(&self, world_entity: &E) -> Option<E> {
        if let Some(entity_record) = self.entity_records.get(world_entity) {
            return entity_record.get_prediction();
        }
        return None;
    }

    pub fn get_confirmed_entity(&self, predicted_entity: &E) -> Option<&E> {
        return self.predicted_to_confirmed_entity.get(predicted_entity);
    }

    pub fn pop_incoming_message(&mut self) -> Option<EntityAction<P, E>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn entity_is_owned(&self, key: &E) -> bool {
        if let Some(entity_record) = self.entity_records.get(key) {
            return entity_record.is_owned();
        }
        return false;
    }

    pub fn prediction_reset_entity<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) {
        if let Some(predicted_entity) = self.get_predicted_entity(&world_entity) {
            // go through all components to make prediction components = world components
            for component_kind in world.get_component_kinds(world_entity) {
                let confirmed_protocol = world.get_component_of_kind(&world_entity, &component_kind)
                    .expect("Predicted and Confirmed entities must always contain the same types of components!")
                    .protocol_copy();
                let mut predicted_protocol: ComponentDynMut<P> = world.get_component_mut_of_kind(&predicted_entity, &component_kind)
                    .expect("Predicted and Confirmed entities must always contain the same types of components!");

                predicted_protocol.mirror(&confirmed_protocol);
            }

            self.queued_incoming_messages
                .push_back(EntityAction::RewindEntity(OwnedEntity::new(
                    world_entity,
                    &predicted_entity,
                )));
        }
    }
}
