use naia_shared::{ReplicaMutTrait, ReplicaRefTrait, Replicate};

// ComponentRef
pub struct ComponentRef<'a, R: Replicate> {
    inner: &'a R,
}

impl<'a, R: Replicate> ComponentRef<'a, R> {
    pub fn new(inner: &'a R) -> Self {
        Self { inner }
    }
}

impl<'a, R: Replicate> ReplicaRefTrait<R> for ComponentRef<'a, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

// ComponentMut
pub struct ComponentMut<'a, R: Replicate> {
    inner: &'a mut R,
}

impl<'a, R: Replicate> ComponentMut<'a, R> {
    pub fn new(inner: &'a mut R) -> Self {
        Self { inner }
    }
}

impl<'a, R: Replicate> ReplicaRefTrait<R> for ComponentMut<'a, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

impl<'a, R: Replicate> ReplicaMutTrait<R> for ComponentMut<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        self.inner
    }
}
