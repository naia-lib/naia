use std::ops::{Deref, DerefMut};

use bevy_ecs::world::Mut as BevyMut;

use naia_shared::{ReplicaDynMutTrait, ReplicaDynRefTrait, Replicate};

// ComponentDynRef
/// Wraps a read-only reference to a concrete `Replicate` component so it
/// can be handed out as a [`naia_shared::ReplicaDynRefTrait`] trait object.
pub struct ComponentDynRef<'a, T>(
    /// The wrapped read-only component reference.
    pub &'a T,
);

impl<'a, R: Replicate> ReplicaDynRefTrait for ComponentDynRef<'a, R> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        #![allow(suspicious_double_ref_op)]
        self.0.deref()
    }
}

// ComponentDynMut
/// Wraps a mutable Bevy component reference so it can be handed out as a
/// [`naia_shared::ReplicaDynMutTrait`] trait object.
pub struct ComponentDynMut<'a, T>(
    /// The wrapped mutable component reference.
    pub BevyMut<'a, T>,
);

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
