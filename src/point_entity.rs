
use std::{
    rc::Rc,
    cell::RefCell,
};
use gaia_shared::{NetEntity, MutHandler, EntityKey, StateMask};
use crate::{ExampleEntity};
use std::borrow::BorrowMut;

#[derive(Clone)]
pub struct PointEntity {
    mut_handler: Option<Rc<RefCell<MutHandler>>>,
    key: Option<EntityKey>,
    x: Option<u8>,
    y: Option<u8>,
}

#[repr(u8)]
enum PointEntityProp {
    X = 0,
    Y = 1,
}

impl PointEntity {
    pub fn init() -> PointEntity {
        //info!("point entity init");
        PointEntity {
            key: None,
            mut_handler: None,
            x: None,
            y: None,
        }
    }

    pub fn new(x: u8, y: u8) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(PointEntity {
            key: None,
            mut_handler: None,
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
        if let Some(mut_handler) = &self.mut_handler {
            if let Some(key) = &self.key {
                mut_handler.as_ref().borrow_mut().mutate(key, prop as u8);
            }
        }
    }
}

impl NetEntity<ExampleEntity> for PointEntity {

    fn get_state_mask_size(&self) -> u8 {
        1
    }

    fn to_type(&self) -> ExampleEntity {
        return ExampleEntity::PointEntity(Rc::new(RefCell::new(self.clone())));
    }

    fn set_mut_handler(&mut self, mut_handler: &Rc<RefCell<MutHandler>>) {
        self.mut_handler = Some(mut_handler.clone());
    }

    fn set_entity_key(&mut self, key: EntityKey) {
        self.key = Some(key);
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

//        info!("entity read() with x: {}, y: {}", self.get_x(), self.get_y());
    }

    fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8]) {
        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
            self.set_x(buffer[0]);
        }
        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
            self.set_y(buffer[1]);
        }

//        info!("entity read_partial() with x: {}, y: {}", self.get_x(), self.get_y());
    }

    fn print(&self, key: u16) {
        info!("entity print(), key: {}, x: {}, y: {}", key, self.get_x(), self.get_y());
    }
}