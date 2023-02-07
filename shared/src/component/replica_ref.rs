use std::ops::{Deref, DerefMut};

use crate::component::replicate::Replicate;

// ReplicaDynRef

pub struct ReplicaDynRef<'b> {
    inner: &'b dyn Replicate,
}

impl<'b> ReplicaDynRef<'b> {
    pub fn new(inner: &'b dyn Replicate) -> Self {
        Self { inner }
    }
}

impl Deref for ReplicaDynRef<'_> {
    type Target = dyn Replicate;

    #[inline]
    fn deref(&self) -> &dyn Replicate {
        self.inner
    }
}

impl<'a> ReplicaDynRefTrait for ReplicaDynRef<'a> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        self.inner
    }
}

// ReplicaDynMut

pub struct ReplicaDynMut<'b> {
    inner: &'b mut dyn Replicate,
}

impl<'b> ReplicaDynMut<'b> {
    pub fn new(inner: &'b mut dyn Replicate) -> Self {
        Self { inner }
    }
}

impl Deref for ReplicaDynMut<'_> {
    type Target = dyn Replicate;

    #[inline]
    fn deref(&self) -> &dyn Replicate {
        self.inner
    }
}

impl DerefMut for ReplicaDynMut<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn Replicate {
        self.inner
    }
}

impl<'a> ReplicaDynRefTrait for ReplicaDynMut<'a> {
    fn to_dyn_ref(&self) -> &dyn Replicate {
        self.inner
    }
}

impl<'a> ReplicaDynMutTrait for ReplicaDynMut<'a> {
    fn to_dyn_mut(&mut self) -> &mut dyn Replicate {
        self.inner
    }
}

// ReplicaRefTrait

pub trait ReplicaRefTrait<R: Replicate> {
    fn to_ref(&self) -> &R;
}

// ReplicaRefWrapper

pub struct ReplicaRefWrapper<'a, R: Replicate> {
    inner: Box<dyn ReplicaRefTrait<R> + 'a>,
}

impl<'a, R: Replicate> ReplicaRefWrapper<'a, R> {
    pub fn new<I: ReplicaRefTrait<R> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, R: Replicate> Deref for ReplicaRefWrapper<'a, R> {
    type Target = R;

    fn deref(&self) -> &R {
        self.inner.to_ref()
    }
}

// ReplicaMutTrait

pub trait ReplicaMutTrait<R: Replicate>: ReplicaRefTrait<R> {
    fn to_mut(&mut self) -> &mut R;
}

// ReplicaMutWrapper

pub struct ReplicaMutWrapper<'a, R: Replicate> {
    inner: Box<dyn ReplicaMutTrait<R> + 'a>,
}

impl<'a, R: Replicate> ReplicaMutWrapper<'a, R> {
    pub fn new<I: ReplicaMutTrait<R> + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a, R: Replicate> Deref for ReplicaMutWrapper<'a, R> {
    type Target = R;

    fn deref(&self) -> &R {
        self.inner.to_ref()
    }
}

impl<'a, R: Replicate> DerefMut for ReplicaMutWrapper<'a, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.inner.to_mut()
    }
}

// ReplicaDynRefWrapper

pub trait ReplicaDynRefTrait {
    fn to_dyn_ref(&self) -> &dyn Replicate;
}

pub struct ReplicaDynRefWrapper<'a> {
    inner: Box<dyn ReplicaDynRefTrait + 'a>,
}

impl<'a> ReplicaDynRefWrapper<'a> {
    pub fn new<I: ReplicaDynRefTrait + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a> Deref for ReplicaDynRefWrapper<'a> {
    type Target = dyn Replicate;

    fn deref(&self) -> &dyn Replicate {
        self.inner.to_dyn_ref()
    }
}

// ReplicaDynMutWrapper

pub trait ReplicaDynMutTrait: ReplicaDynRefTrait {
    fn to_dyn_mut(&mut self) -> &mut dyn Replicate;
}

pub struct ReplicaDynMutWrapper<'a> {
    inner: Box<dyn ReplicaDynMutTrait + 'a>,
}

impl<'a> ReplicaDynMutWrapper<'a> {
    pub fn new<I: ReplicaDynMutTrait + 'a>(inner: I) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl<'a> Deref for ReplicaDynMutWrapper<'a> {
    type Target = dyn Replicate;

    fn deref(&self) -> &dyn Replicate {
        self.inner.to_dyn_ref()
    }
}

impl<'a> DerefMut for ReplicaDynMutWrapper<'a> {
    fn deref_mut(&mut self) -> &mut dyn Replicate {
        self.inner.to_dyn_mut()
    }
}
