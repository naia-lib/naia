use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use log::warn;

use naia_shared::{
    message_list_header,
    serde::{BitReader, Serde, UnsignedVariableInteger},
    BigMap, ChannelIndex, EntityActionType, EntityHandle, EntityHandleConverter,
    FakeEntityConverter, Manifest, NetEntity, NetEntityHandleConverter, Protocolize, Tick,
    WorldMutType,
};

use super::{entity_record::EntityRecord, error::NaiaClientError, event::Event};

pub struct EntityManager<P: Protocolize, E: Copy + Eq + Hash> {
    entity_records: HashMap<E, EntityRecord<P::Kind>>,
    local_to_world_entity: HashMap<NetEntity, E>,
    pub handle_entity_map: BigMap<EntityHandle, E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityManager<P, E> {
    pub fn new() -> Self {
        EntityManager {
            local_to_world_entity: HashMap::new(),
            entity_records: HashMap::new(),
            handle_entity_map: BigMap::new(),
        }
    }

    // Action Reader
    pub fn read_actions<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let action_count = message_list_header::read(reader);
        for _ in 0..action_count {
            self.read_action(
                world,
                manifest,
                server_tick,
                reader,
                event_stream,
            );
        }
    }

    fn read_action<W: WorldMutType<P, E>, C: ChannelIndex>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        server_tick: Tick,
        reader: &mut BitReader,
        event_stream: &mut VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    ) {
        let message_type = EntityActionType::de(reader).unwrap();

        match message_type {
            // Entity Creation
            EntityActionType::SpawnEntity => {
                let net_entity = NetEntity::de(reader).unwrap();
                let components_num = UnsignedVariableInteger::<3>::de(reader).unwrap().get();
                if self.local_to_world_entity.contains_key(&net_entity) {
                    // its possible we received a very late duplicate message
                    warn!("attempted to insert duplicate entity");
                    // continue reading, just don't do anything with the data
                    for _ in 0..components_num {
                        let component_kind = P::Kind::de(reader).unwrap();
                        manifest.create_replica(component_kind, reader, &FakeEntityConverter);
                    }
                } else {
                    // set up entity
                    let world_entity = world.spawn_entity();
                    self.local_to_world_entity.insert(net_entity, world_entity);
                    let entity_handle = self.handle_entity_map.insert(world_entity);
                    self.entity_records
                        .insert(world_entity, EntityRecord::new(net_entity, entity_handle));

                    let mut component_list: Vec<P::Kind> = Vec::new();
                    for _ in 0..components_num {
                        // Component Creation //
                        let component_kind = P::Kind::de(reader).unwrap();

                        let new_component =
                            manifest.create_replica(component_kind, reader, self);

                        component_list.push(component_kind);

                        new_component.extract_and_insert(&world_entity, world);
                        ////////////////////////
                    }

                    let entity_record = self.entity_records.get_mut(&world_entity).unwrap();
                    for component_kind in &component_list {
                        entity_record.component_kinds.insert(*component_kind);
                    }

                    event_stream
                        .push_back(Ok(Event::SpawnEntity(world_entity, component_list)));
                }
            }
            // Entity Deletion
            EntityActionType::DespawnEntity => {
                let net_entity = NetEntity::de(reader).unwrap();
                if let Some(world_entity) = self.local_to_world_entity.remove(&net_entity) {
                    if self.entity_records.remove(&world_entity).is_none() {
                        panic!("despawning an uninitialized entity");
                    }

                    // Generate event for each component, handing references off just in
                    // case
                    for component_kind in world.component_kinds(&world_entity) {
                        if let Some(component) =
                            world.remove_component_of_kind(&world_entity, &component_kind)
                        {
                            event_stream
                                .push_back(Ok(Event::RemoveComponent(world_entity, component)));
                        }
                    }

                    world.despawn_entity(&world_entity);

                    event_stream.push_back(Ok(Event::DespawnEntity(world_entity)));
                }
                warn!("received message attempting to delete nonexistent entity");
            }
            // Add Component to Entity
            EntityActionType::InsertComponent => {
                let net_entity = NetEntity::de(reader).unwrap();
                let component_kind = P::Kind::de(reader).unwrap();

                let new_component = manifest.create_replica(component_kind, reader, self);

                if !self.local_to_world_entity.contains_key(&net_entity) {
                    // its possible we received a very late duplicate message
                    warn!(
                        "attempting to add a component to nonexistent entity: {}",
                        Into::<u16>::into(net_entity)
                    );
                } else {
                    let world_entity = self.local_to_world_entity.get(&net_entity).unwrap();

                    let entity_record = self.entity_records.get_mut(&world_entity).unwrap();

                    entity_record.component_kinds.insert(component_kind);

                    new_component.extract_and_insert(world_entity, world);

                    event_stream
                        .push_back(Ok(Event::InsertComponent(*world_entity, component_kind)));
                }
            }
            // Component Update
            EntityActionType::UpdateComponent => {
                let net_entity = NetEntity::de(reader).unwrap();
                let component_kind = P::Kind::de(reader).unwrap();

                if let Some(world_entity) = self.local_to_world_entity.get(&net_entity) {
                    // read incoming delta
                    world.component_read_partial(world_entity, &component_kind, reader, self);

                    event_stream.push_back(Ok(Event::UpdateComponent(
                        server_tick,
                        *world_entity,
                        component_kind,
                    )));
                } else {
                    panic!("attempting to update component for nonexistent entity");
                }
            }
            // Component Removal
            EntityActionType::RemoveComponent => {
                let net_entity = NetEntity::de(reader).unwrap();
                let component_kind = P::Kind::de(reader).unwrap();

                if let Some(world_entity) = self.local_to_world_entity.get_mut(&net_entity) {
                    if let Some(entity_record) = self.entity_records.get_mut(world_entity) {
                        if entity_record.component_kinds.remove(&component_kind) {
                            // Get component for last change
                            let component = world
                                .remove_component_of_kind(&world_entity, &component_kind)
                                .expect("Component already removed?");

                            // Generate event
                            event_stream.push_back(Ok(Event::RemoveComponent(
                                *world_entity,
                                component,
                            )));
                        } else {
                            panic!("attempting to delete nonexistent component of entity");
                        }
                    } else {
                        panic!("attempting to delete component of nonexistent entity");
                    }
                } else {
                    panic!("attempting to delete component of nonexistent entity");
                }
            }
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityHandleConverter<E> for EntityManager<P, E> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E {
        *self
            .handle_entity_map
            .get(entity_handle)
            .expect("entity does not exist for given handle!")
    }

    fn entity_to_handle(&self, entity: &E) -> EntityHandle {
        self.entity_records
            .get(entity)
            .expect("entity does not exist!")
            .entity_handle
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> NetEntityHandleConverter for EntityManager<P, E> {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity {
        let entity = self
            .handle_entity_map
            .get(entity_handle)
            .expect("no entity exists for the given handle!");
        let entity_record = self.entity_records.get(entity).unwrap();
        return entity_record.net_entity;
    }

    fn net_entity_to_handle(&self, net_entity: &NetEntity) -> EntityHandle {
        let entity = self
            .local_to_world_entity
            .get(net_entity)
            .expect("no entity exists associated with given net entity");
        return self.entity_to_handle(entity);
    }
}
