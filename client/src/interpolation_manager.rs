use std::collections::HashMap;

use crate::client_entity_manager::ClientEntityManager;
use naia_shared::{EntityType, Instant, LocalEntityKey};

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    entity_store: HashMap<LocalEntityKey, (Instant, U, U)>,
    pawn_store: HashMap<LocalEntityKey, (Instant, U, U)>,
    interp_duration: f32,
}

impl<U: EntityType> InterpolationManager<U> {
    pub fn new(tick_duration: &u128) -> Self {
        InterpolationManager {
            entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
            interp_duration: (*tick_duration) as f32,
        }
    }

    // entities
    pub fn create_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        key: &LocalEntityKey,
    ) {
        if let Some(existing_entity) = entity_manager.get_local_entity(key) {
            let temp_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let real_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.entity_store
                .insert(*key, (Instant::now(), temp_entity, real_entity));
        }
    }

    pub fn delete_interpolation(&mut self, key: &LocalEntityKey) {
        self.entity_store.remove(key);
    }

    pub fn get_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        now: &Instant,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some(now_pawn) = entity_manager.get_local_entity(key) {
            if let Some((updated, temp_pawn, old_pawn)) = self.entity_store.get_mut(key) {
                set_smooth(
                    &updated,
                    now,
                    self.interp_duration,
                    temp_pawn,
                    old_pawn,
                    now_pawn,
                );
                return Some(temp_pawn);
            }
        }
        return None;
    }

    pub fn sync_interpolation(&mut self, key: &u16, local_entity: &mut U, now: &Instant) {
        if let Some((updated, _, old_entity)) = self.entity_store.get_mut(key) {
            sync_smooth(
                &updated,
                now,
                self.interp_duration,
                old_entity,
                local_entity,
            );
            updated.set_to(now);
        }
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
            let real_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.pawn_store
                .insert(*key, (Instant::now(), temp_entity, real_entity));
        }
    }

    pub fn delete_pawn_interpolation(&mut self, key: &LocalEntityKey) {
        self.pawn_store.remove(key);
    }

    pub fn get_pawn_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        now: &Instant,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some(now_pawn) = entity_manager.get_pawn(key) {
            if let Some((updated, temp_pawn, old_pawn)) = self.pawn_store.get_mut(key) {
                set_smooth(
                    &updated,
                    now,
                    self.interp_duration,
                    temp_pawn,
                    old_pawn,
                    now_pawn,
                );
                return Some(temp_pawn);
            }
        }
        return None;
    }

    pub fn sync_pawn_interpolation(&mut self, key: &u16, local_entity: &U, now: &Instant) {
        if let Some((updated, _, old_entity)) = self.pawn_store.get_mut(key) {
            sync_smooth(
                &updated,
                now,
                self.interp_duration,
                old_entity,
                local_entity,
            );
            updated.set_to(now);
        }
    }
}

fn set_smooth<U: EntityType>(
    earlier: &Instant,
    now: &Instant,
    interp_duration: f32,
    temp_entity: &mut U,
    old_entity: &mut U,
    now_entity: &U,
) {
    let fraction = get_fraction(earlier, now, interp_duration);
    temp_entity.set_to_interpolation(old_entity, now_entity, fraction);
}

fn sync_smooth<U: EntityType>(
    earlier: &Instant,
    now: &Instant,
    interp_duration: f32,
    old_entity: &mut U,
    now_entity: &U,
) {
    let fraction = get_fraction(earlier, now, interp_duration);
    old_entity.interpolate_with(now_entity, fraction);
}

fn get_fraction(earlier: &Instant, now: &Instant, interp_duration: f32) -> f32 {
    (now.duration_since(earlier).as_millis() as f32).min(interp_duration) / (interp_duration)
}
