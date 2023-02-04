use naia_shared::{ReplicaMutTrait, ReplicaRefTrait, ReplicateSafe};

// ComponentRef
pub struct ComponentRef<'a, R: ReplicateSafe> {
    inner: &'a R,
}

impl<'a, R: ReplicateSafe> ComponentRef<'a, R> {
    pub fn new(inner: &'a R) -> Self {
        Self { inner }
    }
}

impl<'a, R: ReplicateSafe> ReplicaRefTrait<R> for ComponentRef<'a, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

// ComponentMut
pub struct ComponentMut<'a, R: ReplicateSafe> {
    inner: &'a mut R,
}

impl<'a, R: ReplicateSafe> ComponentMut<'a, R> {
    pub fn new(inner: &'a mut R) -> Self {
        Self { inner }
    }
}

impl<'a, R: ReplicateSafe> ReplicaRefTrait<R> for ComponentMut<'a, R> {
    fn to_ref(&self) -> &R {
        self.inner
    }
}

impl<'a, R: ReplicateSafe> ReplicaMutTrait<R> for ComponentMut<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        self.inner
    }
}
