
use std::{
    rc::Rc,
    cell::RefCell,
};

use naia_derive::EntityType;

use crate::{PointEntity};

#[derive(EntityType)]
pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}