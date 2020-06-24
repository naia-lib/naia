
use std::{
    rc::Rc,
    cell::RefCell,
    io::Cursor,
};

use byteorder::{ReadBytesExt};

use super::{
    entity_mutator::EntityMutator,
};
use std::io::Read;

pub trait PropertyIo<T> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>);
    fn write(&self, buffer: &mut Vec<u8>);
}

#[derive(Clone)]
pub struct Property<T> {
    mutator: Option<Rc<RefCell<dyn EntityMutator>>>,
    mutator_index: u8,
    inner: T,
}

impl<T> Property<T> {
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

// IO implementations for common types

impl PropertyIo<u8> for Property<u8> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        self.inner = cursor.read_u8().unwrap();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner);
    }
}

impl PropertyIo<String> for Property<String> {
    fn read(&mut self, cursor: &mut Cursor<&[u8]>) {
        let length = cursor.read_u8().unwrap();
        let buffer = &mut Vec::with_capacity(length as usize);
        for _ in 0..length {
            buffer.push(cursor.read_u8().unwrap());
        }
        self.inner = String::from_utf8_lossy(buffer).to_string();
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.inner.len() as u8);
        let mut bytes = self.inner.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }
}