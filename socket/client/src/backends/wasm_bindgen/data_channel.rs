extern crate log;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{MessageEvent, RtcDataChannel};

// DataChannel
#[derive(Clone)]
pub struct DataChannel {
    pub inner: RtcDataChannel,
}

impl DataChannel {
    pub fn initialize(&mut self) -> Rc<RefCell<VecDeque<Box<[u8]>>>> {

        let message_queue = Rc::new(RefCell::new(VecDeque::new()));
        let message_queue_2 = message_queue.clone();

        let channel_onmsg_func: Box<dyn FnMut(MessageEvent)> =
            Box::new(move |evt: MessageEvent| {
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
        let channel_onmsg_closure = Closure::wrap(channel_onmsg_func);

        self.inner.set_onmessage(Some(channel_onmsg_closure.as_ref().unchecked_ref()));
        channel_onmsg_closure.forget();

        message_queue
    }
}