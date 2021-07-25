use std::collections::HashMap;

use naia_shared::LocalEntityKey;

use super::{locality_status::LocalityStatus, actor_key::ComponentKey};

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    pub components: HashMap<ComponentKey, bool>,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey) -> Self {
        EntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            components: HashMap::new(),
        }
    }
}
