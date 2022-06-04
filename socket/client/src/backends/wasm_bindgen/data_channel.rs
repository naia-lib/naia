extern crate log;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use web_sys::MessagePort;

// DataChannel
#[derive(Clone)]
pub struct DataChannel {
    pub message_port: MessagePort,
    pub message_queue: Rc<RefCell<VecDeque<Box<[u8]>>>>,
}

impl DataChannel {
    pub fn new(message_port: MessagePort) -> Self {
        Self {
            message_port,
            message_queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }
}