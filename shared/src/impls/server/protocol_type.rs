use crate::{
    impls::{Replicate, ReplicateEq},
    protocol_type::{ProtocolKindType, DynRef, DynMut}
};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Sized + Sync + Send + Clone + 'static {
    type Kind: ProtocolKindType;

    /// Get kind of Replicate type
    fn kind_of<R: Replicate<Self>>() -> Self::Kind;
    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> DynRef<'_, Self>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> DynMut<'_, Self>;
    /// Cast to a ReplicateEq impl
    fn cast<R: ReplicateEq<Self>>(self) -> Option<R>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R>;
}