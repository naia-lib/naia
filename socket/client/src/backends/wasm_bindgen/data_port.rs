use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{MessageEvent, MessagePort};

/// A `MessagePort` paired with the queue of inbound messages its `onmessage`
/// handler fills, so the packet receiver can drain it without touching the
/// port directly.
#[derive(Clone)]
pub struct DataPort {
    message_port: MessagePort,
    message_queue: Arc<Mutex<VecDeque<Box<[u8]>>>>,
}

impl DataPort {
    /// Wraps a `MessagePort`, installing an `onmessage` handler that copies
    /// each incoming `ArrayBuffer` into the message queue.
    pub fn new(message_port: MessagePort) -> Self {
        let message_queue = Arc::new(Mutex::new(VecDeque::new()));

        let message_queue_2 = message_queue.clone();
        let port_onmsg_func: Box<dyn FnMut(MessageEvent)> = Box::new(move |evt: MessageEvent| {
            if let Ok(arraybuf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                let uarray: js_sys::Uint8Array = js_sys::Uint8Array::new(&arraybuf);
                let mut body = vec![0; uarray.length() as usize];
                uarray.copy_to(&mut body[..]);
                message_queue_2
                    .lock()
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

    /// Returns a clone of the underlying `MessagePort`.
    pub fn message_port(&self) -> MessagePort {
        self.message_port.clone()
    }

    /// Returns a handle to the queue of messages received so far.
    pub fn message_queue(&self) -> Arc<Mutex<VecDeque<Box<[u8]>>>> {
        self.message_queue.clone()
    }
}
