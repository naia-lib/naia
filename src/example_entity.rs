
use gaia_shared::{EntityType, NetEntity};

use crate::{PointEntity};

#[derive(Clone)]
pub enum ExampleEntity {
    PointEntity(PointEntity),
}

impl EntityType for ExampleEntity {
    fn read(&mut self, bytes: &[u8]) {
        match self {
            ExampleEntity::PointEntity(identity) => {
                identity.read(bytes);
            }
        }
    }
}