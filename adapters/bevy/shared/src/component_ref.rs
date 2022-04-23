use std::ops::{Deref, DerefMut};

use bevy_ecs::world::Mut as BevyMut;

use naia_shared::{
    Protocolize, ReplicaDynMutTrait, ReplicaDynRefTrait, ReplicaMutTrait, ReplicaRefTrait,
    ReplicateSafe,
};

// ComponentRef
pub struct ComponentRef<'a, T>(pub &'a T);

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentRef<'a, R> {
    fn to_ref(&self) -> &R {
        self.0
    }
}

// ComponentMut
pub struct ComponentMut<'a, T>(pub BevyMut<'a, T>);

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentMut<'a, R> {
    fn to_ref(&self) -> &R {
        &self.0
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaMutTrait<P, R> for ComponentMut<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        &mut self.0
    }
}

// ComponentDynRef
pub struct ComponentDynRef<'a, T>(pub &'a T);

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaDynRefTrait<P> for ComponentDynRef<'a, R> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        self.0.deref()
    }
}

// ComponentDynMut
pub struct ComponentDynMut<'a, T>(pub BevyMut<'a, T>);

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaDynRefTrait<P> for ComponentDynMut<'a, R> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        self.0.deref()
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaDynMutTrait<P> for ComponentDynMut<'a, R> {
    fn to_dyn_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        self.0.deref_mut()
    }
}
