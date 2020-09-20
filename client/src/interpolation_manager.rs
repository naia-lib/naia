use std::collections::HashMap;

use crate::client_entity_manager::ClientEntityManager;
use naia_shared::{EntityType, Instant, LocalEntityKey};

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    entity_store: HashMap<LocalEntityKey, (Instant, U)>,
    pawn_store: HashMap<LocalEntityKey, (Instant, U)>,
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
            let copy = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.entity_store.insert(*key, (Instant::now(), copy));
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
        if let Some(tracked_entity) = entity_manager.get_local_entity(key) {
            if let Some((updated, entity)) = self.entity_store.get_mut(key) {
                set_smooth(entity, &updated, tracked_entity, now, self.interp_duration);
                return Some(entity);
            }
        }
        return None;
    }

    pub fn sync_interpolation(&mut self, key: &LocalEntityKey, now: &Instant) {}

    // pawns
    pub fn create_pawn_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        key: &LocalEntityKey,
    ) {
        if let Some(existing_entity) = entity_manager.get_pawn(key) {
            let copy = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            self.pawn_store.insert(*key, (Instant::now(), copy));
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
        if let Some(tracked_pawn) = entity_manager.get_pawn(key) {
            if let Some((updated, pawn)) = self.pawn_store.get_mut(key) {
                set_smooth(pawn, &updated, tracked_pawn, now, self.interp_duration);
                return Some(pawn);
            }
        }
        return None;
    }

    pub fn sync_pawn_interpolation(&mut self, key: &LocalEntityKey, now: &Instant) {}
}

fn set_smooth<U: EntityType>(
    old_entity: &mut U,
    earlier: &Instant,
    now_entity: &U,
    now: &Instant,
    interp_duration: f32,
) {
    // TODO: set old_entity's values to smooth from earlier -> now,
    // current_value -> now_entity

    let fraction =
        (now.duration_since(earlier).as_millis() as f32).min(interp_duration) / (interp_duration);

    old_entity.interpolate_with(now_entity, fraction);
}
