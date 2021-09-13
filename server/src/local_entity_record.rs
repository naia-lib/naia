use naia_shared::{ComponentRecord, LocalEntityKey, Ref};

use super::{keys::component_key::ComponentKey, locality_status::LocalityStatus};

#[derive(Debug)]
pub struct LocalEntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    pub is_prediction: bool,
    component_record: Ref<ComponentRecord<ComponentKey>>,
}

impl LocalEntityRecord {
    pub fn new(
        local_key: LocalEntityKey,
        components_ref: &Ref<ComponentRecord<ComponentKey>>,
    ) -> Self {
        LocalEntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            is_prediction: false,
            component_record: components_ref.clone(),
        }
    }

    pub fn get_component_keys(&self) -> Vec<ComponentKey> {
        return self.component_record.borrow().get_component_keys();
    }
}
