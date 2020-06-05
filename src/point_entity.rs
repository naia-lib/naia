
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
        PointEntity {
            x: None,
            y: None,
        }
    }

    pub fn new(x: u8, y: u8) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(PointEntity {
            x: Some(x),
            y: Some(y),
        }))
    }
}

impl NetEntity<ExampleEntity> for PointEntity {

//    fn write(&self, buffer: &mut Vec<u8>) {
//        match &self.msg {
//            Some(msg_str) => {
//                let mut bytes = msg_str.as_bytes().to_vec();
//                buffer.append(&mut bytes);
//            },
//            None => {}
//        }
//    }
//
    fn read(&mut self, msg: &[u8])  {
//        let msg_str = String::from_utf8_lossy(msg).to_string();
//        self.msg = Some(msg_str);
    }

    fn to_type(&self) -> ExampleEntity {
        return ExampleEntity::PointEntity(self.clone());
    }
}