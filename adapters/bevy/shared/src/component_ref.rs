use std::ops::{Deref, DerefMut};

use bevy_ecs::world::Mut as BevyMut;

use naia_shared::{ReplicaDynMutTrait, ReplicaDynRefTrait, Replicate};

// ComponentDynRef
pub struct ComponentDynRef<'a, T>(pub &'a T);

impl<'a, R: Replicate> ReplicaDynRefTrait for ComponentDynRef<'a, R> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        #![allow(suspicious_double_ref_op)]
        self.0.deref()
    }
}

// ComponentDynMut
pub struct ComponentDynMut<'a, T>(pub BevyMut<'a, T>);

impl<'a, R: Replicate> ReplicaDynRefTrait for ComponentDynMut<'a, R> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        self.0.deref()
    }
}

impl<'a, R: Replicate> ReplicaDynMutTrait for ComponentDynMut<'a, R> {
    fn to_dyn_mut(&mut self) -> &mut dyn Replicate {
        self.0.deref_mut()
    }
}
