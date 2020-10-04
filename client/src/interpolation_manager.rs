use std::collections::HashMap;

use crate::client_entity_manager::ClientEntityManager;
use naia_shared::{sequence_greater_than, wrapping_diff, EntityType, Instant, LocalEntityKey};
use std::time::Duration;

#[derive(Debug)]
pub struct InterpolationManager<U: EntityType> {
    ///////////////////////////////////old_tick, recent_tick, temp_entity, old_entity, prev_entity,
    ///////////////////////////////////old_tick, next_entity, fresh_old
    entity_store: HashMap<LocalEntityKey, (bool, u16, U, U, U, U, bool)>,
    pawn_store: HashMap<LocalEntityKey, (Instant, U, U)>,
    interp_duration: f32,
    tick_duration: Duration,
    last_tick_instant: Instant,
    last_tick: u16,
    fraction: f32,
    accumulator: Duration,
}

impl<U: EntityType> InterpolationManager<U> {
    pub fn new(tick_duration: &Duration, server_tick: u16) -> Self {
        InterpolationManager {
            entity_store: HashMap::new(),
            pawn_store: HashMap::new(),
            tick_duration: (*tick_duration).clone(),
            interp_duration: tick_duration.as_millis() as f32,
            last_tick: server_tick,
            last_tick_instant: Instant::now(),
            accumulator: Duration::new(0, 0),
            fraction: 0.0,
        }
    }

    pub fn mark(&mut self, entity_manager: &ClientEntityManager<U>) {
        self.accumulator += self.last_tick_instant.elapsed();
        self.last_tick_instant = Instant::now();
        if self.accumulator >= self.tick_duration {
            while self.accumulator >= self.tick_duration {
                self.accumulator -= self.tick_duration;
            }
            // tick
            for (key, (fresh_old, _, _, old_ent, prev_ent, next_ent, interp)) in
                self.entity_store.iter_mut()
            {
                if let Some(now_ent) = entity_manager.get_entity(key) {
                    if *fresh_old {
                        *interp = true;
                        *fresh_old = false;
                        prev_ent.mirror(old_ent);
                    } else {
                        *interp = false;
                    }
                    next_ent.mirror(now_ent);
                }
            }
        }
        self.fraction = self.accumulator.as_millis() as f32 / self.interp_duration;
    }

    // entities
    pub fn create_interpolation(
        &mut self,
        entity_manager: &ClientEntityManager<U>,
        key: &LocalEntityKey,
        packet_tick: &u16,
    ) {
        if let Some(existing_entity) = entity_manager.get_entity(key) {
            let temp_entity = existing_entity
                .inner_ref()
                .as_ref()
                .borrow()
                .get_typed_copy();
            let old_entity = existing_entity
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
            self.entity_store.insert(
                *key,
                (
                    false,
                    *packet_tick,
                    temp_entity,
                    old_entity,
                    prev_entity,
                    next_entity,
                    true,
                ),
            );
        }
    }

    pub fn delete_interpolation(&mut self, key: &LocalEntityKey) {
        self.entity_store.remove(key);
    }

    pub fn get_interpolation(&mut self, key: &LocalEntityKey) -> Option<&U> {
        if let Some((_, _, temp_entity, _, prev_entity, next_entity, interp)) =
            self.entity_store.get_mut(key)
        {
            if *interp {
                temp_entity.set_to_interpolation(prev_entity, next_entity, self.fraction);
                return Some(temp_entity);
            } else {
                temp_entity.mirror(next_entity);
                return Some(temp_entity);
            }
        }
        return None;
    }

    pub fn entity_snapshot(&mut self, key: &u16, current_tick: u16, recent_entity: &U) {
        if let Some((fresh_old, recent_tick, _, old_entity, _, _, _)) =
            self.entity_store.get_mut(key)
        {
            if sequence_greater_than(current_tick, *recent_tick) {
                old_entity.mirror(recent_entity);
                *fresh_old = true;
                *recent_tick = current_tick;
            }
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
