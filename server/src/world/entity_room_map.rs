use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::RoomKey;

pub struct EntityRoomMap<E: Copy + Eq + Hash> {
    map: HashMap<E, HashSet<RoomKey>>
}

impl<E: Copy + Eq + Hash> EntityRoomMap<E> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new()
        }
    }

    pub(crate) fn entity_add_room(&mut self, entity: &E, room_key: &RoomKey) {
        if !self.map.contains_key(entity) {
            self.map.insert(*entity, HashSet::new());
        }
        let rooms = self.map.get_mut(entity).unwrap();
        rooms.insert(*room_key);
    }

    pub(crate) fn remove_from_room(&mut self, entity: &E, room_key: &RoomKey) {
        let mut delete = false;
        if let Some(rooms) = self.map.get_mut(entity) {
            rooms.remove(room_key);
            if rooms.is_empty() {
                delete = true;
            }
        }
        if delete {
            self.map.remove(entity);
        }
    }

    pub(crate) fn remove_from_all_rooms(&mut self, entity: &E) -> Option<Vec<RoomKey>> {
        let mut output = Vec::new();

        if let Some(rooms) = self.map.remove(entity) {
            for room in rooms {
                output.push(room);
            }
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }
}
