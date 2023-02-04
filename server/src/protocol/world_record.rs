use std::{collections::HashMap, hash::Hash};

use naia_shared::{BigMap, ComponentId, EntityDoesNotExistError, EntityHandle, EntityHandleConverter};

use crate::{protocol::global_entity_record::GlobalEntityRecord, room::RoomKey};

pub struct WorldRecord<E: Copy + Eq + Hash> {
    /// Information about entities in the internal ECS World
    entity_records: HashMap<E, GlobalEntityRecord>,
    /// Map from the internal [`EntityHandle`] to the external (e.g. Bevy's) entity id
    handle_entity_map: BigMap<EntityHandle, E>,
}

impl<E: Copy + Eq + Hash> Default for WorldRecord<E> {
    fn default() -> Self {
        Self {
            entity_records: HashMap::default(),
            handle_entity_map: BigMap::default(),
        }
    }
}

impl<E: Copy + Eq + Hash> WorldRecord<E> {
    // Sync w/ World & Server

    pub fn spawn_entity(&mut self, entity: &E) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        let entity_handle = self.handle_entity_map.insert(*entity);
        self.entity_records
            .insert(*entity, GlobalEntityRecord::new(entity_handle));
    }

    pub fn despawn_entity(&mut self, entity: &E) -> Option<GlobalEntityRecord> {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }

        self.entity_records.remove(entity)
    }

    pub fn add_component(&mut self, entity: &E, component_type: &ComponentId) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        component_kind_set.insert(*component_type);
    }

    pub fn remove_component(&mut self, entity: &E, component_kind: &ComponentId) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }
    }

    // Access

    pub fn has_entity(&self, entity: &E) -> bool {
        self.entity_records.contains_key(entity)
    }

    pub fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentId>> {
        if !self.entity_records.contains_key(entity) {
            return None;
        }

        let component_kind_set = &self.entity_records.get(entity).unwrap().component_kinds;
        return Some(component_kind_set.iter().copied().collect());
    }

    // Rooms

    pub(crate) fn entity_is_in_room(&self, entity: &E, room_key: &RoomKey) -> bool {
        if let Some(entity_record) = self.entity_records.get(entity) {
            if let Some(actual_room_key) = entity_record.room_key {
                return *room_key == actual_room_key;
            }
        }
        false
    }

    pub(crate) fn entity_enter_room(&mut self, entity: &E, room_key: &RoomKey) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            if entity_record.room_key.is_some() {
                panic!("Entity already belongs to a Room! Remove the Entity from the Room before adding it to a new Room.");
            }
            entity_record.room_key = Some(*room_key);
        }
    }

    pub(crate) fn entity_leave_rooms(&mut self, entity: &E) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            entity_record.room_key = None;
        }
    }
}

impl<E: Copy + Eq + Hash> EntityHandleConverter<E> for WorldRecord<E> {
    fn handle_to_entity(&self, handle: &EntityHandle) -> E {
        return *self
            .handle_entity_map
            .get(handle)
            .expect("should always be an entity for a given handle");
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        if let Some(record) = self.entity_records.get(entity) {
            Ok(record.entity_handle)
        } else {
            Err(EntityDoesNotExistError)
        }
    }
}
