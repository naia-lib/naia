use std::{ops::{DerefMut, Deref}, hash::Hash};

use super::impls::Replicate;

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Sized + Sync + Send + Clone + 'static {
    type Kind: ProtocolKindType;

    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> DynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> DynMut<'_, Self>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R>;
}

pub trait ProtocolKindType: Eq + Hash + Copy {}

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