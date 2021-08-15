use naia_shared::{LocalReplicateKey, Ref, DiffMask};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct ReplicateRecord {
    pub local_key: LocalReplicateKey,
    diff_mask: Ref<DiffMask>,
    pub status: LocalityStatus,
}

impl ReplicateRecord {
    pub fn new(local_key: LocalReplicateKey, diff_mask_size: u8, status: LocalityStatus) -> ReplicateRecord {
        ReplicateRecord {
            local_key,
            diff_mask: Ref::new(DiffMask::new(diff_mask_size)),
            status,
        }
    }

    pub fn get_diff_mask(&self) -> &Ref<DiffMask> {
        return &self.diff_mask;
    }
}
