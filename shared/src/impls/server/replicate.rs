use crate::{diff_mask::DiffMask, property_mutate::PropertyMutator, protocol_type::ProtocolType};

/// A Replica is a Message/Component, or otherwise, a container
/// of Properties that can be scoped, tracked, and synced, with a remote host
pub trait Replicate<P: ProtocolType>: Sync + Send + 'static {

    /// Gets the number of bytes of the Message/Component's DiffMask
    fn get_diff_mask_size(&self) -> u8;
    /// Gets the TypeId of the Message/Component, used to map to a
    /// registered ProtocolType
    fn get_kind(&self) -> P::Kind;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Component on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Component on the client
    fn write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
    /// Set the Message/Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: PropertyMutator);
    /// Returns self
    fn to_protocol(self) -> P;
}

//TODO: do we really need another trait here?
/// Handles equality of Messages/Components.. can't just derive
/// PartialEq because we want to only compare Properties
pub trait ReplicateEq<P: ProtocolType>: Replicate<P> {
    /// Compare with properties in another Replica
    fn equals(&self, other: &Self) -> bool;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &Self);
    /// Gets a copy of the Message/Component
    fn copy(&self) -> Self;
}