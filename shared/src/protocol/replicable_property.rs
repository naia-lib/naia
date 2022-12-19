use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};
use crate::{EntityHandle, NetEntityHandleConverter, PropertyMutator};


/// Trait for types that can be replicated and don't contain entity-related data
pub trait ReplicableProperty {
    /// Inner type that needs to be replicated
    type Inner: Serde;

    /// Create a new Property
    fn new(value: Self::Inner, mutator_index: u8) -> Self;

    /// Set value to the value of another Property, queues for update if value
    /// changes
    fn mirror(&mut self, other: &Self);

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    fn write(&self, writer: &mut dyn BitWrite);

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    fn new_read(reader: &mut BitReader, mutator_index: u8) -> Result<Self, SerdeErr> where Self: Sized;

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr>;

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr>;

    // Comparison

    // TODO: use partial eq instead?
    /// Compare to another property
    fn equals(&self, other: &Self) -> bool;

    // Internal

    /// Set an PropertyMutator to track changes to the Property
    fn set_mutator(&mut self, mutator: &PropertyMutator);
}


/// Trait for types that can be replicated and contain entity-related data
pub trait ReplicableEntityProperty {

    /// Create a new EntityProperty
    fn new(mutator_index: u8) -> Self;

    /// Set value to the value of another Property, queues for update if value
    /// changes
    fn mirror(&mut self, other: &Self);

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter);

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    fn new_read(reader: &mut BitReader, mutator_index: u8, converter: &dyn NetEntityHandleConverter) -> Result<Self, SerdeErr> where Self: Sized;

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr>;

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    fn read(&mut self, reader: &mut BitReader, converter: &dyn NetEntityHandleConverter,) -> Result<(), SerdeErr>;

    // Comparison

    // TODO: use partial eq instead?
    /// Compare to another property
    fn equals(&self, other: &Self) -> bool;

    // Entities

    fn entities(&self) -> Vec<EntityHandle>;

    // Internal

    /// Set an PropertyMutator to track changes to the Property
    fn set_mutator(&mut self, mutator: &PropertyMutator);
}