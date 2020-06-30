use std::{cell::RefCell, rc::Rc};

use naia_shared::{LocalEntityKey, StateMask};

#[derive(Debug)]
pub struct EntityRecord {
    pub local_key: LocalEntityKey,
    state_mask: Rc<RefCell<StateMask>>,
    pub status: LocalEntityStatus,
}

#[derive(Debug, PartialEq)]
pub enum LocalEntityStatus {
    Creating,
    Created,
    Deleting,
}

impl EntityRecord {
    pub fn new(local_key: LocalEntityKey, state_mask_size: u8) -> EntityRecord {
        EntityRecord {
            local_key,
            state_mask: Rc::new(RefCell::new(StateMask::new(state_mask_size))),
            status: LocalEntityStatus::Creating,
        }
    }

    pub fn get_state_mask(&self) -> &Rc<RefCell<StateMask>> {
        return &self.state_mask;
    }
}
