
use std::collections::HashMap;
use crate::{NetTypeTrait, NetEvent};

pub struct Manifest {
    type_count: u32,
    type_map: HashMap<u32, Box<dyn NetTypeTrait>>
}

impl Manifest {
    pub fn new() -> Self {
        Manifest {
            type_count: 0,
            type_map: HashMap::new()
        }
    }

    pub fn register_type(&mut self, net_type_boxed: Box<dyn NetTypeTrait>) {
        self.type_map.insert(self.type_count, net_type_boxed);
        self.type_count += 1;
    }

    pub fn process(&mut self) {

    }
}