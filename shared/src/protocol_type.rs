use std::{ops::{DerefMut, Deref}, hash::Hash};

use super::impls::Replicate;

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Clone + Sync + Send + 'static {
    type Kind: ProtocolKindType;

    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> DynRef<'_, Self, Self::Kind>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> DynMut<'_, Self, Self::Kind>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate>(&mut self) -> Option<&mut R>;
}

pub trait ProtocolKindType: Eq + Hash + Copy {}

// DynRef

pub struct DynRef<'b, P: ProtocolType, K: ProtocolKindType> {
    inner: &'b dyn Replicate<Protocol = P, Kind = K>,
}

impl<'b, P: ProtocolType, K: ProtocolKindType> DynRef<'b, P, K> {
    pub fn new(inner: &'b dyn Replicate<Protocol = P, Kind = K>) -> Self {
        return Self {
            inner
        };
    }
}

impl<P: ProtocolType, K: ProtocolKindType> Deref for DynRef<'_, P, K> {
    type Target = dyn Replicate<Protocol = P, Kind = K>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<Protocol = P, Kind = K> {
        self.inner
    }
}

// DynMut

pub struct DynMut<'b, P: ProtocolType, K: ProtocolKindType> {
    inner: &'b mut dyn Replicate<Protocol = P, Kind = K>,
}

impl<'b, P: ProtocolType, K: ProtocolKindType> DynMut<'b, P, K> {
    pub fn new(inner: &'b mut dyn Replicate<Protocol = P, Kind = K>) -> Self {
        return Self {
            inner
        };
    }
}

impl<P: ProtocolType, K: ProtocolKindType> Deref for DynMut<'_, P, K> {
    type Target = dyn Replicate<Protocol = P, Kind = K>;

    #[inline]
    fn deref(&self) -> &dyn Replicate<Protocol = P, Kind = K> {
        self.inner
    }
}

impl<P: ProtocolType, K: ProtocolKindType> DerefMut for DynMut<'_, P, K> {
    #[inline]
    fn deref_mut(&mut self) -> &mut dyn Replicate<Protocol = P, Kind = K> {
        self.inner
    }
}