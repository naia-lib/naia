use std::ops::{Deref, DerefMut};

use bevy::ecs::world::Mut as BevyMut;

use naia_shared::{
    ProtocolType, ReplicaDynMutTrait, ReplicaDynRefTrait, ReplicaMutTrait, ReplicaRefTrait,
    ReplicateSafe,
};

// ComponentRef
pub struct ComponentRef<'a, T>(pub &'a T);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentRef<'a, R> {
    fn to_ref(&self) -> &R {
        return &self.0;
    }
}

// ComponentMut
pub struct ComponentMut<'a, T>(pub &'a mut T);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentMut<'a, R> {
    fn to_ref(&self) -> &R {
        return &self.0;
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaMutTrait<P, R> for ComponentMut<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        return &mut self.0;
    }
}

// ComponentDynRef
pub struct ComponentDynRef<'a, T>(pub &'a T);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaDynRefTrait<P> for ComponentDynRef<'a, R> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        return self.0.deref();
    }
}

// ComponentDynMut
pub struct ComponentDynMut<'a, T>(pub &'a mut T);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaDynRefTrait<P> for ComponentDynMut<'a, R> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        return self.0.deref();
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaDynMutTrait<P> for ComponentDynMut<'a, R> {
    fn to_dyn_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        return self.0.deref_mut();
    }
}
