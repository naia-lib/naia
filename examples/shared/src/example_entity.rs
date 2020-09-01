use std::{cell::RefCell, rc::Rc};

use naia_derive::EntityType;

use crate::PointEntity;

#[derive(EntityType, Clone)]
pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}
