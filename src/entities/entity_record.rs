use std::{
    rc::Rc,
    cell::RefCell,
};

use crate::{StateMask};

pub struct EntityRecord {
    pub local_key: u16,
    state_mask: Rc<RefCell<StateMask>>,
    pub status: LocalEntityStatus,
}

#[derive(PartialEq)]
pub enum LocalEntityStatus {
    Creating,
    Created,
    Deleting,
}

impl EntityRecord {
    pub fn new(local_key: u16, state_mask_size: u8) -> EntityRecord {
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