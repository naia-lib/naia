use naia_shared::{DiffMask, LocalComponentKey, Ref};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct LocalComponentRecord {
    pub local_key: LocalComponentKey,
    diff_mask: Ref<DiffMask>,
    pub status: LocalityStatus,
}

impl LocalComponentRecord {
    pub fn new(
        local_key: LocalComponentKey,
        diff_mask: &Ref<DiffMask>,
        status: LocalityStatus,
    ) -> LocalComponentRecord {
        LocalComponentRecord {
            local_key,
            diff_mask: diff_mask.clone(),
            status,
        }
    }

    pub fn get_diff_mask(&self) -> &Ref<DiffMask> {
        return &self.diff_mask;
    }
}
