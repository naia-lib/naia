use naia_shared::{LocalActorKey, Ref, StateMask};

use super::locality_status::LocalityStatus;

#[derive(Debug)]
pub struct ActorRecord {
    pub local_key: LocalActorKey,
    state_mask: Ref<StateMask>,
    pub status: LocalityStatus,
}

impl ActorRecord {
    pub fn new(local_key: LocalActorKey, state_mask_size: u8) -> ActorRecord {
        ActorRecord {
            local_key,
            state_mask: Ref::new(StateMask::new(state_mask_size)),
            status: LocalityStatus::Creating,
        }
    }

    pub fn get_state_mask(&self) -> &Ref<StateMask> {
        return &self.state_mask;
    }
}
