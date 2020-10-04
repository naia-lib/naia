use std::collections::HashMap;

use crate::{client_entity_manager::ClientEntityManager, client_tick_manager::ClientTickManager};
use naia_shared::{EntityType, LocalEntityKey};
use std::time::Duration;

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    ////////temp_entity, prev_entity, next_entity
    entity_store: HashMap<LocalEntityKey, (U, U, U)>,
    pawn_store: HashMap<LocalEntityKey, (U, U, U)>,
    interp_duration: f32,
}

impl<U: EntityType> InterpolationManager<U> {
    pub fn new(tick_duration: &Duration) -> Self {
        InterpolationManager {
            entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
            interp_duration: tick_duration.as_millis() as f32,
        }
    }

    pub fn tick(&mut self, entity_manager: &ClientEntityManager<U>) {
        for (key, (_, prev_ent, next_ent)) in self.entity_store.iter_mut() {
            if let Some(now_ent) = entity_manager.get_entity(key) {
                prev_ent.mirror(next_ent);
                next_ent.mirror(now_ent);
            }
        }

        for (key, (_, prev_ent, next_ent)) in self.pawn_store.iter_mut() {
            if let Some(now_ent) = entity_manager.get_pawn(key) {
                prev_ent.mirror(next_ent);
                next_ent.mirror(now_ent);
            }
        }
    }

    // entities
    pub fn create_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        key: &LocalEntityKey,
    ) {
        if let Some(existing_entity) = entity_manager.get_entity(key) {
            let temp_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let prev_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let next_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.entity_store
                .insert(*key, (temp_entity, prev_entity, next_entity));
        }
    }

    pub fn delete_interpolation(&mut self, key: &LocalEntityKey) {
        self.entity_store.remove(key);
    }

    pub fn get_interpolation(
        &mut self,
        tick_manager: &ClientTickManager,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some((temp_entity, prev_entity, next_entity)) = self.entity_store.get_mut(key) {
            temp_entity.set_to_interpolation(prev_entity, next_entity, tick_manager.fraction);
            return Some(temp_entity);
        }
        return None;
    }

    // pawns
    pub fn create_pawn_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        key: &LocalEntityKey,
    ) {
        if let Some(existing_entity) = entity_manager.get_pawn(key) {
            let temp_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let prev_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let next_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.pawn_store
                .insert(*key, (temp_entity, prev_entity, next_entity));
        }
    }

    pub fn delete_pawn_interpolation(&mut self, key: &LocalEntityKey) {
        self.pawn_store.remove(key);
    }

    pub fn get_pawn_interpolation(
        &mut self,
        tick_manager: &ClientTickManager,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some((temp_entity, prev_entity, next_entity)) = self.pawn_store.get_mut(key) {
            temp_entity.set_to_interpolation(prev_entity, next_entity, tick_manager.fraction);
            return Some(temp_entity);
        }
        return None;
    }
}
