use std::collections::{HashMap, HashSet};

use naia_shared::GlobalEntity;

use crate::user::UserKey;

pub struct EntityScopeMap {
    entities_of_user: HashMap<UserKey, HashSet<GlobalEntity>>,
    users_of_entity: HashMap<GlobalEntity, HashSet<UserKey>>,
    main_map: HashMap<(UserKey, GlobalEntity), bool>,
    /// Per-`(user, entity)` one-shot override: treat the pair's next scope
    /// exit as [`ScopeExit::Despawn`] even when the entity's own
    /// `ReplicationConfig` says `Persist`.
    ///
    /// Lifetime is exactly one exit → re-entry cycle. The pair is removed
    /// when the override fires (`take_despawn_on_next_exit` at an exit site)
    /// **and** when the entity is next included back into that user's scope
    /// (`clear_despawn_on_next_exit` at a re-entry site), so a stale
    /// revocation can never replay against a later legitimate re-seed.
    ///
    /// Purged alongside `main_map` by `remove_user`/`remove_entity` — an
    /// override must not outlive the pair it refers to.
    despawn_on_next_exit: HashSet<(UserKey, GlobalEntity)>,
}

impl EntityScopeMap {
    pub fn new() -> Self {
        Self {
            main_map: HashMap::new(),
            entities_of_user: HashMap::new(),
            users_of_entity: HashMap::new(),
            despawn_on_next_exit: HashSet::new(),
        }
    }

    pub fn get(&self, user_key: &UserKey, entity: &GlobalEntity) -> Option<&bool> {
        let key = (*user_key, *entity);

        self.main_map.get(&key)
    }

    pub fn insert(&mut self, user_key: UserKey, entity: GlobalEntity, in_scope: bool) {
        self.entities_of_user.entry(user_key).or_default();
        self.users_of_entity.entry(entity).or_default();

        self.entities_of_user
            .get_mut(&user_key)
            .unwrap()
            .insert(entity);
        self.users_of_entity
            .get_mut(&entity)
            .unwrap()
            .insert(user_key);

        self.main_map.insert((user_key, entity), in_scope);
    }

    /// Arms the one-shot scope-exit override for `(user_key, entity)`.
    ///
    /// Idempotent — arming an already-armed pair changes nothing, so a caller
    /// that re-runs its policy every tick cannot stack revocations.
    pub fn set_despawn_on_next_exit(&mut self, user_key: &UserKey, entity: &GlobalEntity) {
        self.despawn_on_next_exit.insert((*user_key, *entity));
    }

    /// Consumes the override for `(user_key, entity)`, returning whether it
    /// was armed. Called at a scope-exit site: firing disarms.
    pub fn take_despawn_on_next_exit(&mut self, user_key: &UserKey, entity: &GlobalEntity) -> bool {
        self.despawn_on_next_exit.remove(&(*user_key, *entity))
    }

    /// Disarms the override for `(user_key, entity)` without firing it.
    /// Called at a scope re-entry site — the entity is back in the user's
    /// scope, so the cycle this override belonged to is over.
    pub fn clear_despawn_on_next_exit(&mut self, user_key: &UserKey, entity: &GlobalEntity) {
        self.despawn_on_next_exit.remove(&(*user_key, *entity));
    }

    /// Returns whether the override is currently armed for `(user_key, entity)`.
    ///
    /// The read surface of the override ledger; the engine itself only ever
    /// consumes (`take_`) or disarms (`clear_`), so this is used by tests.
    #[allow(dead_code)]
    pub fn has_despawn_on_next_exit(&self, user_key: &UserKey, entity: &GlobalEntity) -> bool {
        self.despawn_on_next_exit.contains(&(*user_key, *entity))
    }

    pub fn remove_user(&mut self, user_key: &UserKey) {
        if let Some(entities) = self.entities_of_user.get(user_key) {
            for entity in entities {
                if let Some(users) = self.users_of_entity.get_mut(entity) {
                    users.remove(user_key);
                    self.main_map.remove(&(*user_key, *entity));
                }
            }
        }
        self.despawn_on_next_exit
            .retain(|(user, _)| user != user_key);

        self.entities_of_user.remove(user_key);
    }

    pub fn remove_entity(&mut self, entity: &GlobalEntity) {
        self.despawn_on_next_exit.retain(|(_, ent)| ent != entity);
        if let Some(users) = self.users_of_entity.get(entity) {
            for user in users {
                if let Some(entities) = self.entities_of_user.get_mut(user) {
                    entities.remove(entity);
                    self.main_map.remove(&(*user, *entity));
                }
            }
        }

        self.users_of_entity.remove(entity);
    }
}

#[cfg(test)]
mod despawn_on_next_exit_tests {
    use naia_shared::BigMapKey;

    use super::*;

    fn entity(id: u64) -> GlobalEntity {
        GlobalEntity::from_u64(id)
    }

    #[test]
    fn arming_is_per_pair_and_idempotent() {
        let mut map = EntityScopeMap::new();
        let (u, v) = (UserKey::from_u64(1), UserKey::from_u64(2));
        let (a, b) = (entity(1), entity(2));

        map.set_despawn_on_next_exit(&u, &a);
        map.set_despawn_on_next_exit(&u, &a);

        assert!(map.has_despawn_on_next_exit(&u, &a));
        assert!(!map.has_despawn_on_next_exit(&v, &a));
        assert!(!map.has_despawn_on_next_exit(&u, &b));

        // Idempotent: one arming, one firing.
        assert!(map.take_despawn_on_next_exit(&u, &a));
        assert!(!map.take_despawn_on_next_exit(&u, &a));
    }

    #[test]
    fn clearing_disarms_without_firing() {
        let mut map = EntityScopeMap::new();
        let u = UserKey::from_u64(1);
        let a = entity(1);

        map.set_despawn_on_next_exit(&u, &a);
        map.clear_despawn_on_next_exit(&u, &a);
        assert!(!map.has_despawn_on_next_exit(&u, &a));
        assert!(!map.take_despawn_on_next_exit(&u, &a));

        // Clearing an unarmed pair is a no-op, not a panic.
        map.clear_despawn_on_next_exit(&u, &a);
    }

    #[test]
    fn removing_a_user_purges_only_that_user_s_overrides() {
        let mut map = EntityScopeMap::new();
        let (u, v) = (UserKey::from_u64(1), UserKey::from_u64(2));
        let a = entity(1);

        map.insert(u, a, true);
        map.insert(v, a, true);
        map.set_despawn_on_next_exit(&u, &a);
        map.set_despawn_on_next_exit(&v, &a);

        map.remove_user(&u);

        assert!(!map.has_despawn_on_next_exit(&u, &a));
        assert!(map.has_despawn_on_next_exit(&v, &a));
    }

    #[test]
    fn removing_an_entity_purges_only_that_entity_s_overrides() {
        let mut map = EntityScopeMap::new();
        let u = UserKey::from_u64(1);
        let (a, b) = (entity(1), entity(2));

        map.insert(u, a, true);
        map.insert(u, b, true);
        map.set_despawn_on_next_exit(&u, &a);
        map.set_despawn_on_next_exit(&u, &b);

        map.remove_entity(&a);

        assert!(!map.has_despawn_on_next_exit(&u, &a));
        assert!(map.has_despawn_on_next_exit(&u, &b));
    }

    #[test]
    fn a_fresh_map_arms_nothing() {
        let mut map = EntityScopeMap::new();
        let u = UserKey::from_u64(1);
        assert!(!map.has_despawn_on_next_exit(&u, &entity(1)));
        assert!(!map.take_despawn_on_next_exit(&u, &entity(1)));
    }
}
