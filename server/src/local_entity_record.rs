use naia_shared::NetEntity;

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct LocalEntityRecord {
    pub entity_net_id: NetEntity,
    pub status: LocalityStatus,
}

impl LocalEntityRecord {
    pub fn new(entity_net_id: NetEntity) -> Self {
        LocalEntityRecord {
            entity_net_id,
            status: LocalityStatus::Creating,
        }
    }
}
