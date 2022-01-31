use naia_shared::EntityNetId;

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct LocalEntityRecord {
    pub entity_net_id: EntityNetId,
    pub status: LocalityStatus,
}

impl LocalEntityRecord {
    pub fn new(entity_net_id: EntityNetId) -> Self {
        LocalEntityRecord {
            entity_net_id,
            status: LocalityStatus::Creating,
        }
    }
}
