use std::collections::HashMap;

use crate::client_entity_manager::ClientEntityManager;
use naia_shared::{EntityType, Instant, LocalEntityKey};
use std::time::Duration;

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    ///////////////////////////////////old_tick, recent_tick, temp_entity, old_entity, prev_entity,
    ///////////////////////////////////old_tick, next_entity, fresh_old
    entity_store: HashMap<LocalEntityKey, (U, U, U)>,
    pawn_store: HashMap<LocalEntityKey, (U, U, U)>,
    interp_duration: f32,
    tick_duration: f32,
    last_tick_instant: Instant,
    last_tick: u16,
    fraction: f32,
    accumulator: f32,
}

impl<U: EntityType> InterpolationManager<U> {
    pub fn new(tick_duration: &Duration, server_tick: u16) -> Self {
        InterpolationManager {
            entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
            tick_duration: tick_duration.as_nanos() as f32 / 1000000000.0,
            interp_duration: tick_duration.as_millis() as f32,
            last_tick: server_tick,
            last_tick_instant: Instant::now(),
            accumulator: 0.0,
            fraction: 0.0,
        }
    }

    pub fn mark(&mut self, entity_manager: &ClientEntityManager<U>) {
        let mut frame_time = self.last_tick_instant.elapsed().as_nanos() as f32 / 1000000000.0;
        if frame_time > 0.25 {
            frame_time = 0.25;
        }
        self.accumulator += frame_time;
        self.last_tick_instant = Instant::now();
        if self.accumulator >= self.tick_duration {
            while self.accumulator >= self.tick_duration {
                self.accumulator -= self.tick_duration;
            }
            // tick
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
        self.fraction = self.accumulator / self.tick_duration;
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

    pub fn get_interpolation(&mut self, key: &LocalEntityKey) -> Option<&U> {
        if let Some((temp_entity, prev_entity, next_entity)) = self.entity_store.get_mut(key) {
            temp_entity.set_to_interpolation(prev_entity, next_entity, self.fraction);
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

    pub fn get_pawn_interpolation(&mut self, key: &LocalEntityKey) -> Option<&U> {
        if let Some((temp_entity, prev_entity, next_entity)) = self.pawn_store.get_mut(key) {
            temp_entity.set_to_interpolation(prev_entity, next_entity, self.fraction);
            return Some(temp_entity);
        }
        return None;
    }
}
