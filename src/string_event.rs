
use gaia_shared::{Event};
use crate::ExampleEvent;

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

impl Event<ExampleEvent> for StringEvent {
    fn is_guaranteed(&self) -> bool {
        true
    }

    fn to_type(&self) -> ExampleEvent {
        return ExampleEvent::StringEvent(self.clone());
    }

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