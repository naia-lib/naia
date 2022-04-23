use std::marker::PhantomData;

use naia_shared::{Protocolize, ReplicaMutTrait, ReplicaRefTrait, ReplicateSafe};

// ComponentRef
pub struct ComponentRef<'a, P: Protocolize, R: ReplicateSafe<P>> {
    inner: &'a R,
    phantom: PhantomData<P>,
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ComponentRef<'a, P, R> {
    pub fn new(inner: &'a R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentRef<'a, P, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

// ComponentMut
pub struct ComponentMut<'a, P: Protocolize, R: ReplicateSafe<P>> {
    inner: &'a mut R,
    phantom: PhantomData<P>,
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ComponentMut<'a, P, R> {
    pub fn new(inner: &'a mut R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaRefTrait<P, R> for ComponentMut<'a, P, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaMutTrait<P, R> for ComponentMut<'a, P, R> {
    fn to_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}
