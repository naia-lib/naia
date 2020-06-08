use crate::{LocalEntityKey, StateMask};

pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    state_mask: StateMask,
    pub status: LocalEntityStatus,
}

pub enum LocalEntityStatus {
    Creating,
    Created,
    Deleting,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey, state_mask_size: u8) -> EntityRecord {
        EntityRecord {
            local_key,
            state_mask: StateMask::new(state_mask_size),
            status: LocalEntityStatus::Creating,
        }
    }
}