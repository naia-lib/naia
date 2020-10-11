use std::{cell::RefCell, rc::Rc};

use naia_shared::{LocalActorKey, StateMask};

#[derive(Debug)]
pub struct ActorRecord {
    pub local_key: LocalActorKey,
    state_mask: Rc<RefCell<StateMask>>,
    pub status: LocalActorStatus,
}

#[derive(Debug, PartialEq)]
pub enum LocalActorStatus {
    Creating,
    Created,
    Deleting,
}

impl ActorRecord {
    pub fn new(local_key: LocalActorKey, state_mask_size: u8) -> ActorRecord {
        ActorRecord {
            local_key,
            state_mask: Rc::new(RefCell::new(StateMask::new(state_mask_size))),
            status: LocalActorStatus::Creating,
        }
    }

    pub fn get_state_mask(&self) -> &Rc<RefCell<StateMask>> {
        return &self.state_mask;
    }
}
