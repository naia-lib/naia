use naia_shared::{LocalEntityKey, Ref, ComponentRecord};

use super::{keys::ComponentKey, locality_status::LocalityStatus};

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    component_record: Ref<ComponentRecord<ComponentKey>>,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey, components_ref: &Ref<ComponentRecord<ComponentKey>>) -> Self {
        EntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            component_record: components_ref.clone(),
        }
    }

    pub fn get_component_keys(&self) -> Vec<ComponentKey> {
        return self.component_record.borrow().get_component_keys();
    }
}
