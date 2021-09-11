use naia_shared::{DiffMask, LocalComponentKey, Ref};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct ReplicaRecord {
    pub local_key: LocalComponentKey,
    diff_mask: Ref<DiffMask>,
    pub status: LocalityStatus,
}

impl ReplicaRecord {
    pub fn new(
        local_key: LocalComponentKey,
        diff_mask_size: u8,
        status: LocalityStatus,
    ) -> ReplicaRecord {
        ReplicaRecord {
            local_key,
            diff_mask: Ref::new(DiffMask::new(diff_mask_size)),
            status,
        }
    }

    pub fn get_diff_mask(&self) -> &Ref<DiffMask> {
        return &self.diff_mask;
    }
}
