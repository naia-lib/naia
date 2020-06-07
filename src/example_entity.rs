
use gaia_shared::{EntityType};

use crate::{PointEntity};

#[derive(Clone)]
pub enum ExampleEntity {
    PointEntity(PointEntity),
}

impl EntityType for ExampleEntity {

}