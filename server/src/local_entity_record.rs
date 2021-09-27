use naia_shared::{LocalEntityKey, Ref};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct LocalEntityRecord {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    pub is_prediction: bool,
}

impl LocalEntityRecord {
    pub fn new(
        local_key: LocalEntityKey,
    ) -> Self {
        LocalEntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            is_prediction: false,
        }
    }
}
