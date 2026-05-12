use std::ops::{Deref, DerefMut};

use crate::world::component::replicate::Replicate;

/// Shared reference to a type-erased replicated component.
pub struct ReplicaDynRef<'b> {
    inner: &'b dyn Replicate,
}

impl<'b> ReplicaDynRef<'b> {
    /// Creates a `ReplicaDynRef` wrapping the given reference.
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

/// Mutable reference to a type-erased replicated component.
pub struct ReplicaDynMut<'b> {
    inner: &'b mut dyn Replicate,
}

impl<'b> ReplicaDynMut<'b> {
    /// Creates a `ReplicaDynMut` wrapping the given mutable reference.
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

/// Trait for typed shared access to a concrete replicated component.
pub trait ReplicaRefTrait<R: Replicate> {
    /// Returns a shared reference to the concrete component.
    fn to_ref(&self) -> &R;
}

/// Type-erasing wrapper around a `ReplicaRefTrait<R>` implementation.
pub struct ReplicaRefWrapper<'a, R: Replicate> {
    inner: Box<dyn ReplicaRefTrait<R> + 'a>,
}

impl<'a, R: Replicate> ReplicaRefWrapper<'a, R> {
    /// Creates a `ReplicaRefWrapper` from any `ReplicaRefTrait<R>` implementor.
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

/// Trait for typed mutable access to a concrete replicated component.
pub trait ReplicaMutTrait<R: Replicate>: ReplicaRefTrait<R> {
    /// Returns a mutable reference to the concrete component.
    fn to_mut(&mut self) -> &mut R;
}

/// Type-erasing wrapper around a `ReplicaMutTrait<R>` implementation.
pub struct ReplicaMutWrapper<'a, R: Replicate> {
    inner: Box<dyn ReplicaMutTrait<R> + 'a>,
}

impl<'a, R: Replicate> ReplicaMutWrapper<'a, R> {
    /// Creates a `ReplicaMutWrapper` from any `ReplicaMutTrait<R>` implementor.
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

/// Trait for shared access to a type-erased replicated component.
pub trait ReplicaDynRefTrait {
    /// Returns a shared `dyn Replicate` reference.
    fn to_dyn_ref(&self) -> &dyn Replicate;
}

/// Type-erasing wrapper for a `ReplicaDynRefTrait` implementor.
pub struct ReplicaDynRefWrapper<'a> {
    inner: Box<dyn ReplicaDynRefTrait + 'a>,
}

impl<'a> ReplicaDynRefWrapper<'a> {
    /// Creates a `ReplicaDynRefWrapper` from any `ReplicaDynRefTrait` implementor.
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

/// Trait for mutable access to a type-erased replicated component.
pub trait ReplicaDynMutTrait: ReplicaDynRefTrait {
    /// Returns a mutable `dyn Replicate` reference.
    fn to_dyn_mut(&mut self) -> &mut dyn Replicate;
}

/// Type-erasing wrapper for a `ReplicaDynMutTrait` implementor.
pub struct ReplicaDynMutWrapper<'a> {
    inner: Box<dyn ReplicaDynMutTrait + 'a>,
}

impl<'a> ReplicaDynMutWrapper<'a> {
    /// Creates a `ReplicaDynMutWrapper` from any `ReplicaDynMutTrait` implementor.
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
