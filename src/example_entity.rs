
use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_shared::{EntityType, Entity, StateMask};

use crate::{PointEntity};

pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}

impl EntityType for ExampleEntity {
//    fn read(&mut self, bytes: &[u8]) {
//        match self {
//            ExampleEntity::PointEntity(identity) => {
//                identity.as_ref().borrow_mut().read(bytes);
//            }
//        }
//    }

    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]) {
        match self {
            ExampleEntity::PointEntity(identity) => {
                identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
            }
        }
    }

    fn print(&self, key: u16) {
        match self {
            ExampleEntity::PointEntity(identity) => {
                identity.as_ref().borrow().print(key);
            }
        }
    }

//    fn init(&self) -> Self {
//        match self {
//            ExampleEntity::PointEntity(_) => {
//                return ExampleEntity::PointEntity(Rc::new(RefCell::new(PointEntity::init())));
//            }
//        }
//    }

    fn clone_inner_rc(&self) -> Self {
        match self {
            ExampleEntity::PointEntity(identity) => {
                return ExampleEntity::PointEntity(identity.clone());
            }
        }
    }
}