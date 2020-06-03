
use gaia_shared::{NetBase, NetEvent};
use crate::ExampleType;

#[derive(Clone)]
pub struct StringEvent {
    msg: Option<String>,
}

impl StringEvent {
    pub fn init() -> StringEvent {
        StringEvent {
            msg: None,
        }
    }

    pub fn new(msg: String) -> Self {
        StringEvent {
            msg: Some(msg)
        }
    }

    pub fn get_message(&self) -> Option<String> {
        match &self.msg {
            Some(inner) => {
                Some(inner.clone())
            }
            None => None
        }
    }
}

impl NetBase<ExampleType> for StringEvent {
    fn to_type(self) -> ExampleType {
        return ExampleType::StringEvent(self);
    }
    fn is_event(&self) -> bool {
        true
    }
}

impl NetEvent<ExampleType> for StringEvent {
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

    fn read(&mut self, msg: &[u8]) {
        let msg_str = String::from_utf8_lossy(msg).to_string();
        self.msg = Some(msg_str);
    }
}