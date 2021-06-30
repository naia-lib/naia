use naia_derive::ActorType;

use naia_shared::Ref;

use crate::PointActor;

#[derive(ActorType, Clone)]
pub enum ExampleActor {
    PointActor(Ref<PointActor>),
}
