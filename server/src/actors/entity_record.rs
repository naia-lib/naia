use naia_shared::LocalEntityKey;

use super::locality_status::LocalActorStatus;

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalActorStatus,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey) -> Self {
        EntityRecord {
            local_key,
            status: LocalActorStatus::Creating,
        }
    }
}
