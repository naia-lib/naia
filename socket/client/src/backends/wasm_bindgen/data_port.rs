extern crate log;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use web_sys::MessagePort;

// DataChannel
#[derive(Clone)]
pub struct DataPort {
    message_port: MessagePort,
    message_queue: Rc<RefCell<VecDeque<Box<[u8]>>>>,
}

impl DataPort {
    pub fn new(message_port: MessagePort) -> Self {
        Self {
            message_port,
            message_queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    pub fn message_port(&self) -> MessagePort {
        self.message_port.clone()
    }

    pub fn message_queue(&self) -> Rc<RefCell<VecDeque<Box<[u8]>>>> {
        self.message_queue.clone()
    }
}