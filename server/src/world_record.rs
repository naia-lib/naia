use std::{
    collections::HashMap,
    hash::Hash,
};

use naia_shared::{BigMap, EntityHandle, EntityHandleConverter, EntityHandleInner, ProtocolKindType};

use super::{global_entity_record::GlobalEntityRecord, room::RoomKey};

pub struct WorldRecord<E: Copy + Eq + Hash, K: ProtocolKindType> {
    entity_records: HashMap<E, GlobalEntityRecord<K>>,
    handle_entity_map: BigMap<EntityHandleInner, E>,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> WorldRecord<E, K> {
    pub fn new() -> Self {
        Self {
            entity_records: HashMap::new(),
            handle_entity_map: BigMap::new(),
        }
    }

    // Sync w/ World & Server

    pub fn spawn_entity(&mut self, entity: &E) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        self.entity_records.insert(*entity, GlobalEntityRecord::new());
    }

    pub fn despawn_entity(&mut self, entity: &E) -> Option<GlobalEntityRecord<K>> {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }

        return self.entity_records.remove(entity);
    }

    pub fn add_component(&mut self, entity: &E, component_type: &K) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        component_kind_set.insert(*component_type);
    }

    pub fn remove_component(&mut self, entity: &E, component_kind: &K) {
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
        return self.entity_records.contains_key(entity);
    }

    pub fn component_kinds(&self, entity: &E) -> Vec<K> {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }

        let component_kind_set = &self.entity_records.get(entity).unwrap().component_kinds;
        return component_kind_set.iter().map(|kind_ref| *kind_ref).collect();
    }

    // Rooms

    pub(crate) fn entity_is_in_room(&self, entity: &E, room_key: &RoomKey) -> bool {
        if let Some(entity_record) = self.entity_records.get(entity) {
            if let Some(actual_room_key) = entity_record.room_key {
                return *room_key == actual_room_key;
            }
        }
        return false;
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

impl<E: Copy + Eq + Hash, K: ProtocolKindType> EntityHandleConverter<E> for WorldRecord<E, K> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Option<&E> {
        if let Some(inner_entity_handle) = entity_handle.inner() {
            return self.handle_entity_map.get(inner_entity_handle);
        }
        return None;
    }

    fn entity_to_handle(&mut self, entity: &E) -> EntityHandle {
        let entity_record = self
            .entity_records
            .get_mut(entity)
            .expect("entity does not exist!");
        if entity_record.entity_handle.is_none() {
            entity_record.entity_handle = Some(self.handle_entity_map.insert(*entity));
        }

        entity_record.entity_handle.unwrap().to_outer()
    }
}
