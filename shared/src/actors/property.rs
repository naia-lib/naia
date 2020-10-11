use std::{cell::RefCell, rc::Rc};

use super::actor_mutator::ActorMutator;

/// A Property of an Actor, that contains data which must be tracked for
/// updates, and synced to the Client
#[derive(Clone)]
pub struct Property<T: Clone + PartialEq> {
    mutator: Option<Rc<RefCell<dyn ActorMutator>>>,
    mutator_index: u8,
    pub(crate) inner: T,
    pub(crate) last_recv_index: u16,
}

impl<T: Clone + PartialEq> Property<T> {
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
}
