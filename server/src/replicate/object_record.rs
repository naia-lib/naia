use naia_shared::{LocalObjectKey, Ref, DiffMask};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct ObjectRecord {
    pub local_key: LocalObjectKey,
    diff_mask: Ref<DiffMask>,
    pub status: LocalityStatus,
}

impl ObjectRecord {
    pub fn new(local_key: LocalObjectKey, diff_mask_size: u8, status: LocalityStatus) -> ObjectRecord {
        ObjectRecord {
            local_key,
            diff_mask: Ref::new(DiffMask::new(diff_mask_size)),
            status,
        }
    }

    pub fn get_diff_mask(&self) -> &Ref<DiffMask> {
        return &self.diff_mask;
    }
}
