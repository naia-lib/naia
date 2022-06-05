extern crate log;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{MessageEvent, MessagePort};

// DataChannel
#[derive(Clone)]
pub struct DataPort {
    message_port: MessagePort,
    message_queue: Rc<RefCell<VecDeque<Box<[u8]>>>>,
}

impl DataPort {
    pub fn new(message_port: MessagePort) -> Self {
        let message_queue = Rc::new(RefCell::new(VecDeque::new()));

        let message_queue_2 = message_queue.clone();
        let port_onmsg_func: Box<dyn FnMut(MessageEvent)> = Box::new(move |evt: MessageEvent| {
            if let Ok(arraybuf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                let uarray: js_sys::Uint8Array = js_sys::Uint8Array::new(&arraybuf);
                let mut body = vec![0; uarray.length() as usize];
                uarray.copy_to(&mut body[..]);
                message_queue_2
                    .try_borrow_mut()
                    .expect("can't borrow 'message_queue_2' to retrieve message!")
                    .push_back(body.into_boxed_slice());
            }
        });
        let port_onmsg_closure = Closure::wrap(port_onmsg_func);

        message_port.set_onmessage(Some(port_onmsg_closure.as_ref().unchecked_ref()));
        port_onmsg_closure.forget();

        Self {
            message_port,
            message_queue,
        }
    }

    pub fn message_port(&self) -> MessagePort {
        self.message_port.clone()
    }

    pub fn message_queue(&self) -> Rc<RefCell<VecDeque<Box<[u8]>>>> {
        self.message_queue.clone()
    }
}
