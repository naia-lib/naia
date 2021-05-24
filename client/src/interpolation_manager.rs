use std::collections::HashMap;

use crate::{client_actor_manager::ClientActorManager, client_tick_manager::ClientTickManager};
use naia_shared::{ActorType, LocalActorKey};
use std::time::Duration;

#[derive(Debug)]
pub struct InterpolationManager<U: ActorType> {
    ////////temp_actor, prev_actor, next_actor
    actor_store: HashMap<LocalActorKey, (U, U)>,
    pawn_store: HashMap<LocalActorKey, (U, U, U)>,
    interp_duration: f32,
}

impl<U: ActorType> InterpolationManager<U> {
    pub fn new(tick_duration: &Duration) -> Self {
        InterpolationManager {
            actor_store: HashMap::new(),
            pawn_store: HashMap::new(),
            interp_duration: tick_duration.as_millis() as f32,
        }
    }

    pub fn update_actors(&mut self, actor_manager: &ClientActorManager<U>) {
        for (key, (_, prev_ent)) in self.actor_store.iter_mut() {
            if let Some(now_ent) = actor_manager.get_actor(key) {
                prev_ent.mirror(now_ent);
            }
        }
    }

    pub fn update_pawns(&mut self, actor_manager: &ClientActorManager<U>) {
        for (key, (_, prev_ent, next_ent)) in self.pawn_store.iter_mut() {
            if let Some(now_ent) = actor_manager.get_pawn(key) {
                prev_ent.mirror(next_ent);
                next_ent.mirror(now_ent);
            }
        }
    }

    // actors
    pub fn create_interpolation(
        &mut self,
        actor_manager: &ClientActorManager<U>,
        key: &LocalActorKey,
    ) {
        if let Some(existing_actor) = actor_manager.get_actor(key) {
            let temp_actor = existing_actor
                .inner_ref()
                .borrow()
                .get_typed_copy();
            let prev_actor = existing_actor
                .inner_ref()
                .borrow()
                .get_typed_copy();
            self.actor_store.insert(*key, (temp_actor, prev_actor));
        }
    }

    pub fn delete_interpolation(&mut self, key: &LocalActorKey) {
        self.actor_store.remove(key);
    }

    pub fn get_interpolation(
        &mut self,
        tick_manager: &ClientTickManager,
        actor_manager: &ClientActorManager<U>,
        key: &LocalActorKey,
    ) -> Option<&U> {
        if let Some((temp_actor, prev_actor)) = self.actor_store.get_mut(key) {
            if let Some(next_actor) = actor_manager.get_actor(key) {
                temp_actor.set_to_interpolation(prev_actor, next_actor, tick_manager.fraction);
                return Some(temp_actor);
            }
        }
        return None;
    }

    // pawns
    pub fn create_pawn_interpolation(
        &mut self,
        actor_manager: &ClientActorManager<U>,
        key: &LocalActorKey,
    ) {
        if let Some(existing_actor) = actor_manager.get_pawn(key) {
            let temp_actor = existing_actor
                .inner_ref()
                .borrow()
                .get_typed_copy();
            let prev_actor = existing_actor
                .inner_ref()
                .borrow()
                .get_typed_copy();
            let next_actor = existing_actor
                .inner_ref()
                .borrow()
                .get_typed_copy();
            self.pawn_store
                .insert(*key, (temp_actor, prev_actor, next_actor));
        }
    }

    pub fn delete_pawn_interpolation(&mut self, key: &LocalActorKey) {
        self.pawn_store.remove(key);
    }

    pub fn get_pawn_interpolation(
        &mut self,
        tick_manager: &ClientTickManager,
        key: &LocalActorKey,
    ) -> Option<&U> {
        if let Some((temp_actor, prev_actor, next_actor)) = self.pawn_store.get_mut(key) {
            temp_actor.set_to_interpolation(prev_actor, next_actor, tick_manager.fraction);
            return Some(temp_actor);
        }
        return None;
    }
}
