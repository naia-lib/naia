use naia_server::{ProtocolType, ImplRef, Replicate};

use crate::world::entity::Entity;

use super::{server::Server, commands::{Insert, Remove, Despawn}};

// EntityMut

pub struct EntityMut<'a, 'b, P: ProtocolType> {
    entity: Entity,
    commands: &'b mut Server<'a, P>,
}

impl<'a, 'b, P: ProtocolType> EntityMut<'a, 'b, P> {

    pub fn new(entity: Entity, commands: &'b mut Server<'a, P>) -> Self {
        return EntityMut {
            entity, commands,
        };
    }

    #[inline]
    pub fn id(&self) -> Entity {
        self.entity
    }

    pub fn insert<R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {
        self.commands.add(Insert::new(
            self.entity,
            component_ref.clone_ref(),
        ));
        self
    }

    pub fn remove<R: Replicate<P>>(&mut self) -> &mut Self
    {
        self.commands.add(Remove::<P, R>::new(
            self.entity
        ));
        self
    }

    pub fn despawn(&mut self) {
        self.commands.add(Despawn::new(
            self.entity,
        ))
    }

    pub fn commands(&mut self) -> &mut Server<'a, P> {
        self.commands
    }
}