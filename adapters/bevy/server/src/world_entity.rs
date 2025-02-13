use bevy_ecs::{system::Resource, entity::Entity};

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub(crate) struct WorldEntity {
    world_id: WorldId,
    entity: Entity,
}

impl WorldEntity {

    pub fn main_new(entity: Entity) -> Self {
        Self { world_id: WorldId::main(), entity }
    }

    // pub fn sub_new(world_id: WorldId, entity: Entity) -> Self {
    //     if world_id.is_main() {
    //         panic!("WorldEntity::sub_new: world_id must be a sub-world id");
    //     }
    //     Self { world_id, entity }
    // }

    pub fn new(world_id: WorldId, entity: Entity) -> Self {
        Self { world_id, entity }
    }

    pub fn world_id(&self) -> WorldId {
        self.world_id
    }

    pub fn entity(&self) -> Entity {
        self.entity
    }

    // pub fn world_is_main(&self) -> bool {
    //     self.world_id.is_main()
    // }
}

#[derive(Resource, Clone, Copy, Eq, PartialEq, Hash)]
pub struct WorldId(Option<u16>);

impl WorldId {

    pub fn main() -> Self {
        Self(None)
    }

    pub fn sub(id: u16) -> Self {
        Self(Some(id))
    }

    pub fn is_main(&self) -> bool {
        self.0.is_none()
    }
}

