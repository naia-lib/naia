
use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_derive::EntityType;

use gaia_shared::{EntityType, Entity, StateMask};

use crate::{PointEntity};

#[derive(EntityType)]
pub enum ExampleEntity {
    PointEntity(Rc<RefCell<PointEntity>>),
}