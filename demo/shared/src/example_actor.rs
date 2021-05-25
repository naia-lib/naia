use naia_derive::ActorType;

use crate::{PointActor, Ref};

#[derive(ActorType, Clone)]
pub enum ExampleActor {
    PointActor(Ref<PointActor>),
}
