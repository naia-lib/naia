use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use hecs::{Entity as HecsEntity, World as HecsWorld};

use naia_server::{ImplRef, EntityType, ProtocolType, Ref, Replicate, WorldType};

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity(HecsEntity);

impl Entity {
    pub fn new(entity: HecsEntity) -> Self {
        return Entity(entity);
    }
}

impl EntityType for Entity {}