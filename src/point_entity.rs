
use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_shared::{Entity, StateMask, EntityMutator};

use crate::{ExampleEntity};

pub struct PointEntity {
    mutator: Option<Rc<RefCell<dyn EntityMutator>>>,
    x: Option<u8>,
    y: Option<u8>,
}

#[repr(u8)]
enum PointEntityProp {
    X = 0,
    Y = 1,
}

pub struct PointEntityBuilder {
}

impl PointEntityBuilder {

}

impl PointEntity {
    pub fn init() -> PointEntity {
        //info!("point entity init");
        PointEntity {
            mutator: None,
            x: None,
            y: None,
        }
    }

    pub fn new(x: u8, y: u8) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(PointEntity {
            mutator: None,
            x: Some(x),
            y: Some(y),
        }))
    }

    pub fn get_x(&self) -> u8 {
        if let Some(x) = self.x {
            return x;
        }
        return 0;
    }

    pub fn get_y(&self) -> u8 {
        if let Some(y) = self.y {
            return y;
        }
        return 0;
    }

    pub fn set_x(&mut self, value: u8) {
        self.x = Some(value);
        self.notify_mutation(PointEntityProp::X);
    }

    pub fn set_y(&mut self, value: u8) {
        self.y = Some(value);
        self.notify_mutation(PointEntityProp::Y);
    }

    pub fn step(&mut self) {
        let mut x = self.get_x();
        x += 1;
        if x > 20 {
            x = 0;
        }
        if x % 3 == 0 {
            let mut y = self.get_y();
            y += 1;
            self.set_y(y);
        }
        self.set_x(x);
    }

    fn notify_mutation(&mut self, prop: PointEntityProp) {
        if let Some(mutator) = &self.mutator {
            mutator.as_ref().borrow_mut().mutate(prop as u8);
        }
    }
}

impl Entity<ExampleEntity> for PointEntity {

    fn get_state_mask_size(&self) -> u8 {
        1
    }

    //to_type COPIES the current entity,
    fn to_type(&self) -> ExampleEntity {
        let copied_entity = PointEntity::new(self.get_x(), self.get_y());
        return ExampleEntity::PointEntity(copied_entity);
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.get_x());
        buffer.push(self.get_y());
    }

    fn write_partial(&self, state_mask_ref: &Rc<RefCell<StateMask>>, buffer: &mut Vec<u8>) {
        let state_mask = state_mask_ref.as_ref().borrow();
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            buffer.push(self.get_x());
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            buffer.push(self.get_y());
        }
    }

    fn read(&mut self, buffer: &[u8])  {
        self.set_x(buffer[0]);
        self.set_y(buffer[1]);
    }

    fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8]) {
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            self.set_x(buffer[0]);
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            self.set_y(buffer[1]);
        }
    }

    fn print(&self, key: u16) {
        info!("entity print(), key: {}, x: {}, y: {}", key, self.get_x(), self.get_y());
    }

    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
        self.mutator = Some(mutator.clone());
    }
}