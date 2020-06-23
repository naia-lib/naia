
use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_shared::{EntityType, Entity, StateMask};

use crate::{PointEntity};

//TODO: Candidate for Macro (just list names of entity structs ("PointEntity")
pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}

impl EntityType for ExampleEntity {

    //TODO: Candidate for Macro
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]) {
        match self {
            ExampleEntity::PointEntity(identity) => {
                identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
            }
        }
    }

    //TODO: Candidate for Macro
    fn clone_inner_rc(&self) -> Self {
        match self {
            ExampleEntity::PointEntity(identity) => {
                return ExampleEntity::PointEntity(identity.clone());
            }
        }
    }
}