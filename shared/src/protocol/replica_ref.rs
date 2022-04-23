use std::ops::{Deref, DerefMut};

use super::{protocolize::Protocolize, replicate::ReplicateSafe};

// ReplicaDynRef

pub struct ReplicaDynRef<'b, P: Protocolize> {
    inner: &'b dyn ReplicateSafe<P>,
}

impl<'b, P: Protocolize> ReplicaDynRef<'b, P> {
    pub fn new(inner: &'b dyn ReplicateSafe<P>) -> Self {
        Self { inner }
    }
}

impl<P: Protocolize> Deref for ReplicaDynRef<'_, P> {
    type Target = dyn ReplicateSafe<P>;

    #[inline]
    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

impl<'a, P: Protocolize> ReplicaDynRefTrait<P> for ReplicaDynRef<'a, P> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

// ReplicaDynMut

pub struct ReplicaDynMut<'b, P: Protocolize> {
    inner: &'b mut dyn ReplicateSafe<P>,
}

impl<'b, P: Protocolize> ReplicaDynMut<'b, P> {
    pub fn new(inner: &'b mut dyn ReplicateSafe<P>) -> Self {
        Self { inner }
    }
}

impl<P: Protocolize> Deref for ReplicaDynMut<'_, P> {
    type Target = dyn ReplicateSafe<P>;

    #[inline]
    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

impl<P: Protocolize> DerefMut for ReplicaDynMut<'_, P> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        self.inner
    }
}

impl<'a, P: Protocolize> ReplicaDynRefTrait<P> for ReplicaDynMut<'a, P> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P> {
        self.inner
    }
}

impl<'a, P: Protocolize> ReplicaDynMutTrait<P> for ReplicaDynMut<'a, P> {
    fn to_dyn_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        self.inner
    }
}

// ReplicaRefTrait

pub trait ReplicaRefTrait<P: Protocolize, R: ReplicateSafe<P>> {
    fn to_ref(&self) -> &R;
}

// ReplicaRefWrapper

pub struct ReplicaRefWrapper<'a, P: Protocolize, R: ReplicateSafe<P>> {
    inner: Box<dyn ReplicaRefTrait<P, R> + 'a>,
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaRefWrapper<'a, P, R> {
    pub fn new<I: ReplicaRefTrait<P, R> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> Deref for ReplicaRefWrapper<'a, P, R> {
    type Target = R;

    fn deref(&self) -> &R {
        self.inner.to_ref()
    }
}

// ReplicaMutTrait

pub trait ReplicaMutTrait<P: Protocolize, R: ReplicateSafe<P>>: ReplicaRefTrait<P, R> {
    fn to_mut(&mut self) -> &mut R;
}

// ReplicaMutWrapper

pub struct ReplicaMutWrapper<'a, P: Protocolize, R: ReplicateSafe<P>> {
    inner: Box<dyn ReplicaMutTrait<P, R> + 'a>,
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> ReplicaMutWrapper<'a, P, R> {
    pub fn new<I: ReplicaMutTrait<P, R> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> Deref for ReplicaMutWrapper<'a, P, R> {
    type Target = R;

    fn deref(&self) -> &R {
        self.inner.to_ref()
    }
}

impl<'a, P: Protocolize, R: ReplicateSafe<P>> DerefMut for ReplicaMutWrapper<'a, P, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.inner.to_mut()
    }
}

// ReplicaDynRefWrapper

pub trait ReplicaDynRefTrait<P: Protocolize> {
    fn to_dyn_ref(&self) -> &dyn ReplicateSafe<P>;
}

pub struct ReplicaDynRefWrapper<'a, P: Protocolize> {
    inner: Box<dyn ReplicaDynRefTrait<P> + 'a>,
}

impl<'a, P: Protocolize> ReplicaDynRefWrapper<'a, P> {
    pub fn new<I: ReplicaDynRefTrait<P> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: Protocolize> Deref for ReplicaDynRefWrapper<'a, P> {
    type Target = dyn ReplicateSafe<P>;

    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner.to_dyn_ref()
    }
}

// ReplicaDynMutWrapper

pub trait ReplicaDynMutTrait<P: Protocolize>: ReplicaDynRefTrait<P> {
    fn to_dyn_mut(&mut self) -> &mut dyn ReplicateSafe<P>;
}

pub struct ReplicaDynMutWrapper<'a, P: Protocolize> {
    inner: Box<dyn ReplicaDynMutTrait<P> + 'a>,
}

impl<'a, P: Protocolize> ReplicaDynMutWrapper<'a, P> {
    pub fn new<I: ReplicaDynMutTrait<P> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, P: Protocolize> Deref for ReplicaDynMutWrapper<'a, P> {
    type Target = dyn ReplicateSafe<P>;

    fn deref(&self) -> &dyn ReplicateSafe<P> {
        self.inner.to_dyn_ref()
    }
}

impl<'a, P: Protocolize> DerefMut for ReplicaDynMutWrapper<'a, P> {
    fn deref_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        self.inner.to_dyn_mut()
    }
}
