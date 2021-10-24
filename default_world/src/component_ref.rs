use std::marker::PhantomData;

use naia_shared::{ProtocolType, ReplicaMutTrait, ReplicaRefTrait, ReplicateSafe};

// RefWrapper
pub struct RefWrapper<'a, P: ProtocolType, R: ReplicateSafe<P>> {
    inner: &'a R,
    phantom: PhantomData<P>,
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> RefWrapper<'a, P, R> {
    pub fn new(inner: &'a R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for RefWrapper<'a, P, R> {
    fn to_ref(&self) -> &R {
        return &self.inner;
    }
}

// MutWrapper
pub struct MutWrapper<'a, P: ProtocolType, R: ReplicateSafe<P>> {
    inner: &'a mut R,
    phantom: PhantomData<P>,
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> MutWrapper<'a, P, R> {
    pub fn new(inner: &'a mut R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for MutWrapper<'a, P, R> {
    fn to_ref(&self) -> &R {
        return &self.inner;
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ReplicaMutTrait<P, R> for MutWrapper<'a, P, R> {
    fn to_mut(&mut self) -> &mut R {
        return &mut self.inner;
    }
}

// ComponentDynRef
//pub type ComponentDynRef<'a, P> = ReplicaDynRef<'a, P>;

// ComponentDynMut
//pub type ComponentDynMut<'a, P> = ReplicaDynMut<'a, P>;
