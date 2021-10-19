use std::collections::{HashMap, HashSet};

use naia_shared::EntityType;

use crate::user::user_key::UserKey;

pub struct EntityScopeMap<E: EntityType> {
    entities_of_user: HashMap<UserKey, HashSet<E>>,
    users_of_entity: HashMap<E, HashSet<UserKey>>,
    main_map: HashMap<(UserKey, E), bool>,
}

impl<E: EntityType> EntityScopeMap<E> {
    pub fn new() -> Self {
        Self {
            main_map: HashMap::new(),
            entities_of_user: HashMap::new(),
            users_of_entity: HashMap::new(),
        }
    }

    pub fn get(&self, user_key: &UserKey, entity: &E) -> Option<&bool> {
        let key = (*user_key, *entity);

        return self.main_map.get(&key);
    }

    pub fn insert(&mut self, user_key: UserKey, entity: E, in_scope: bool) {
        if !self.entities_of_user.contains_key(&user_key) {
            self.entities_of_user.insert(user_key, HashSet::new());
        }
        if !self.users_of_entity.contains_key(&entity) {
            self.users_of_entity.insert(entity, HashSet::new());
        }

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

    pub fn remove_user(&mut self, user_key: &UserKey) {
        if let Some(entities) = self.entities_of_user.get(user_key) {
            for entity in entities {
                if let Some(users) = self.users_of_entity.get_mut(entity) {
                    users.remove(user_key);
                    self.main_map.remove(&(*user_key, *entity));
                }
            }
        }

        self.entities_of_user.remove(user_key);
    }

    pub fn remove_entity(&mut self, entity: &E) {
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
