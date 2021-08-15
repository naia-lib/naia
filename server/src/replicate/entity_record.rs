use std::collections::HashSet;

use naia_shared::{LocalEntityKey, Ref};

use super::{keys::ComponentKey, locality_status::LocalityStatus};

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    pub components_ref: Ref<HashSet<ComponentKey>>,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey, components_ref: &Ref<HashSet<ComponentKey>>) -> Self {
        EntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            components_ref: components_ref.clone(),
        }
    }
}
