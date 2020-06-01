
use gaia_shared::{NetBase, NetEvent};

#[derive(Clone)]
pub struct ExampleEvent {
    msg: Option<String>,
}

impl ExampleEvent {
    pub fn init() -> Self {
        ExampleEvent {
            msg: None,
        }
    }

    pub fn new(msg: String) -> Self {
        ExampleEvent {
            msg: Some(msg)
        }
    }
}

impl NetBase for ExampleEvent {
}

impl NetEvent for ExampleEvent {
//    fn is_guaranteed() -> bool {
//        false
//    }
//
    fn write(&self, buffer: &mut Vec<u8>) {
        match &self.msg {
            Some(msg_str) => {
                let mut bytes = msg_str.as_bytes().to_vec();
                buffer.append(&mut bytes);
            },
            None => {}
        }
    }
//
//    fn read(mut msg: &[u8]) -> Self {
//        let msg_str = String::from_utf8_lossy(msg).to_string();
//        ExampleEvent {
//            msg: msg_str
//        }
//    }
}