
use gaia_shared::NetEvent;

struct ExampleEvent {
    msg: String,
}

impl ExampleEvent {
    pub fn new(msg: &str) -> Self {
        ExampleEvent {
            msg: msg.into_string()
        }
    }
}

impl NetEvent for ExampleEvent {
    fn is_guaranteed(&self) -> bool {
        false
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        unimplemented!()
    }

    fn read(mut msg: &[u8]) -> Self {
        unimplemented!()
    }
}