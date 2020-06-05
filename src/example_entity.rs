
use gaia_shared::{EntityType};

use crate::{PointEntity};

pub enum ExampleEntity {
    PointEntity(PointEntity),
}

impl EntityType for ExampleEntity {

}