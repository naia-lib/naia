use naia_shared::Ref;

use naia_derive::ActorType;

use crate::PointActor;

#[derive(ActorType, Clone)]
pub enum ExampleActor {
    PointActor(Ref<PointActor>),
}
