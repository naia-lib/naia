use std::any::TypeId;

use naia_socket_shared::PacketReader;

use crate::EntityType;

use super::{diff_mask::DiffMask, replicate::Replicate};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Clone + Sync + Send + 'static {
    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> &dyn Replicate<Self>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> &mut dyn Replicate<Self>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R>;
}

pub trait ProtocolKindType {}