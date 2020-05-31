
use gaia_shared::{NetBase, NetEvent};

pub struct ExampleEvent {
    msg: Option<String>,
}

impl ExampleEvent {
    pub fn new(msg: &str) -> Self {
        ExampleEvent {
            msg: Some(msg.to_string())
        }
    }
}

impl NetBase for ExampleEvent {
    fn identity() -> Box<Self> {
        Box::new(ExampleEvent { msg: None, })
    }
}

impl NetEvent for ExampleEvent {
//    fn is_guaranteed() -> bool {
//        false
//    }
//
//    fn write(&self, buffer: &mut Vec<u8>) {
//        let mut bytes = self.msg.into_bytes();
//        buffer.append(&mut bytes);
//    }
//
//    fn read(mut msg: &[u8]) -> Self {
//        let msg_str = String::from_utf8_lossy(msg).to_string();
//        ExampleEvent {
//            msg: msg_str
//        }
//    }
}