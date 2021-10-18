use naia_shared::LocalComponentKey;

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct LocalComponentRecord {
    pub local_key: LocalComponentKey,
    pub status: LocalityStatus,
}

impl LocalComponentRecord {
    pub fn new(
        local_key: LocalComponentKey,
        status: LocalityStatus,
    ) -> LocalComponentRecord {
        LocalComponentRecord {
            local_key,
            status,
        }
    }
}
