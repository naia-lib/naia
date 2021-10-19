use std::{ops::{DerefMut, Deref}, hash::Hash};

use super::{entity_type::EntityType, impls::{Replicate, ProtocolType}};

pub trait ProtocolKindType: Eq + Hash + Copy {
    fn to_u16(&self) -> u16;
    fn from_u16(val: u16) -> Self;
}

pub trait ProtocolExtractor<P: ProtocolType, N: EntityType> {
    fn extract<R: Replicate<P>>(&mut self, entity: &N, component: R);
}

// DynRef

pub struct DynRef<'b, P: ProtocolType> {
    inner: &'b dyn Replicate<P>,
}

impl<'b, P: ProtocolType> DynRef<'b, P> {
    pub fn new(inner: &'b dyn Replicate<P>) -> Self {
        return Self {
            inner
        };
    }
}

impl<P: ProtocolType> Deref for DynRef<'_, P> {
    type Target = dyn Replicate<P>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner
    }
}

// DynMut

pub struct DynMut<'b, P: ProtocolType> {
    inner: &'b mut dyn Replicate<P>,
}

impl<'b, P: ProtocolType> DynMut<'b, P> {
    pub fn new(inner: &'b mut dyn Replicate<P>) -> Self {
        return Self {
            inner
        };
    }
}

impl<P: ProtocolType> Deref for DynMut<'_, P> {
    type Target = dyn Replicate<P>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<P> {
        self.inner
    }
}

impl<P: ProtocolType> DerefMut for DynMut<'_, P> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn Replicate<P> {
        self.inner
    }
}