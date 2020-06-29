use std::{cell::RefCell, rc::Rc};

use super::entity_mutator::EntityMutator;

#[derive(Clone)]
pub struct Property<T: Clone> {
    mutator: Option<Rc<RefCell<dyn EntityMutator>>>,
    mutator_index: u8,
    pub(crate) inner: T,
}

impl<T: Clone> Property<T> {
    pub fn new(value: T, index: u8) -> Property<T> {
        return Property::<T> {
            inner: value,
            mutator_index: index,
            mutator: None,
        };
    }

    pub fn get(&self) -> &T {
        return &self.inner;
    }

    pub fn set(&mut self, value: T) {
        self.inner = value;
        if let Some(mutator) = &self.mutator {
            mutator.as_ref().borrow_mut().mutate(self.mutator_index);
        }
    }

    pub fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
        self.mutator = Some(mutator.clone());
    }
}
