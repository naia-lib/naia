use nanoserde::{DeBin, SerBin};

use naia_socket_shared::PacketReader;

use crate::{property_mutate::PropertyMutate, wrapping_number::sequence_greater_than};

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Clone + DeBin + SerBin + PartialEq + Default> {
    inner: T,
    last_recv_index: u16,
}

// should be shared
impl<T: Clone + DeBin + SerBin + PartialEq + Default> Property<T> {
    /// Gets a reference to the value contained by the Property
    pub fn get(&self) -> &T {
        return &self.inner;
    }

    /// Compare to another property
    pub fn equals(&self, other: &Property<T>) -> bool {
        return self.inner == other.inner;
    }

    /// Set value to the value of another Property
    pub fn mirror(&mut self, other: &Property<T>) {
        self.inner = other.inner.clone();
    }

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, buffer: &mut Vec<u8>) {
        let encoded = &mut SerBin::serialize_bin(&self.inner);
        buffer.push(encoded.len() as u8);
        buffer.append(encoded);
    }
}

// client-specific
impl<T: Clone + DeBin + SerBin + PartialEq + Default> Property<T> {
    /// Create a new Property
    pub fn new(value: T, index: u8) -> Property<T> {
        return Property::<T> {
            inner: value,
            last_recv_index: 0,
        };
    }

    /// Given a cursor into incoming packet data, initializes the Property with the
    /// synced value
    pub fn new_read(reader: &mut PacketReader, packet_index: u16) -> Self {
        let length = reader.read_u8();

        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let start: usize = cursor.position() as usize;
        let end: usize = start + (length as usize);

        let inner =
            DeBin::deserialize_bin(&buffer[start..end]).expect("error deserializing property");

        cursor.set_position(end as u64);

        return Property::<T> {
            inner,
            last_recv_index: packet_index,
        };
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value, but only if data is newer than the last data received
    pub fn read(&mut self, reader: &mut PacketReader, packet_index: u16) {
        let length = reader.read_u8();

        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let start: usize = cursor.position() as usize;
        let end: usize = start + (length as usize);

        if sequence_greater_than(packet_index, self.last_recv_index) {
            self.last_recv_index = packet_index;
            self.inner =
                DeBin::deserialize_bin(&buffer[start..end]).expect("error deserializing property");
        }

        cursor.set_position(end as u64);
    }

    /// Gets a mutable reference to the value contained by the Property, queue
    /// to update
    pub fn get_mut(&mut self) -> &mut T {
        return &mut self.inner;
    }

    /// Set the Property's contained value
    pub fn set(&mut self, value: T) {
        self.inner = value;
    }
}