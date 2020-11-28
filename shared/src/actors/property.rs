use std::{cell::RefCell, rc::Rc};

use nanoserde::{DeBin, SerBin};

use crate::{PacketReader, wrapping_number::sequence_greater_than};

use super::actor_mutator::ActorMutator;

/// A Property of an Actor, that contains data which must be tracked for
/// updates, and synced to the Client
#[derive(Clone)]
pub struct Property<T: Clone + DeBin + SerBin + PartialEq> {
    mutator: Option<Rc<RefCell<dyn ActorMutator>>>,
    mutator_index: u8,
    pub(crate) inner: T,
    pub(crate) last_recv_index: u16,
}

impl<T: Clone + DeBin + SerBin + PartialEq> Property<T> {
    /// Create a new Property
    pub fn new(value: T, index: u8) -> Property<T> {
        return Property::<T> {
            inner: value,
            mutator_index: index,
            mutator: None,
            last_recv_index: 0,
        };
    }

    /// Gets a reference to the value contained by the Property
    pub fn get(&self) -> &T {
        return &self.inner;
    }

    /// Set the Property's contained value
    pub fn set(&mut self, value: T) {
        if let Some(mutator) = &self.mutator {
            mutator.as_ref().borrow_mut().mutate(self.mutator_index);
        }
        self.inner = value;
    }

    /// Set an ActorMutator object to track changes to the Property
    pub fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn ActorMutator>>) {
        self.mutator = Some(mutator.clone());
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
            self.inner = DeBin::deserialize_bin(&buffer[start..end]).unwrap();
        }

        cursor.set_position(end as u64);
    }
}
