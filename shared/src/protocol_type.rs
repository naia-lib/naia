use std::hash::Hash;

use super::replicate::Replicate;

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Clone + Sync + Send + 'static {
    type Kind: ProtocolKindType;

    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> &dyn Replicate<Protocol = Self, Kind = Self::Kind>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> &mut dyn Replicate<Protocol = Self, Kind = Self::Kind>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate>(&mut self) -> Option<&mut R>;
}

pub trait ProtocolKindType: Eq + Hash {}