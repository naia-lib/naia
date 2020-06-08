
use std::{
    rc::Rc,
    cell::RefCell,
};
use gaia_shared::{NetEntity};
use crate::{ExampleEntity};

#[derive(Clone)]
pub struct PointEntity {
    x: Option<u8>,
    y: Option<u8>,
}

impl PointEntity {
    pub fn init() -> PointEntity {
        //info!("point entity init");
        PointEntity {
            x: None,
            y: None,
        }
    }

    pub fn new(x: u8, y: u8) -> Rc<RefCell<Self>> {
        //info!("point entity new");
        Rc::new(RefCell::new(PointEntity {
            x: Some(x),
            y: Some(y),
        }))
    }

    pub fn get_x(&self) -> u8 {
        if let Some(x) = self.x {
            //info!("point entity get_x: {}", x);
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
        //info!("point entity set x");
        self.x = Some(value);
    }

    pub fn set_y(&mut self, value: u8) {
        self.y = Some(value);
    }

    pub fn step(&mut self) {
        let mut x = self.get_x();
        x += 1;
        if x > 10 {
            x = 0;
        }
        self.set_x(x);
        //info!("point entity step: {}", x);
    }
}

impl NetEntity<ExampleEntity> for PointEntity {

    fn get_state_mask_size(&self) -> u8 {
        1
    }

    fn to_type(&self) -> ExampleEntity {
        return ExampleEntity::PointEntity(self.clone());
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.get_x());
        buffer.push(self.get_y());
    }

    fn read(&mut self, buffer: &[u8])  {
        self.set_x(buffer[0]);
        self.set_y(buffer[1]);
    }
}