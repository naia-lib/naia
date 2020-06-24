
use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_derive::EntityType;

use crate::{PointEntity};

#[derive(EntityType)]
pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}