use naia_shared::LocalEntityKey;

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey) -> Self {
        EntityRecord {
            local_key,
            status: LocalityStatus::Creating,
        }
    }
}
