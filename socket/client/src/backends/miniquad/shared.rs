#[no_mangle]
pub extern "C" fn naia_socket_crate_version() -> u32 {
    let major = dbg!(env!("CARGO_PKG_VERSION_MAJOR").parse::<u32>().unwrap());
    let minor = env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap();
    let patch = dbg!(env!("CARGO_PKG_VERSION_PATCH").parse::<u32>().unwrap());

    (major << 24) + (minor << 16) + patch
}

use std::collections::VecDeque;

use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};

// Static vars
pub static mut MESSAGE_QUEUE: Option<VecDeque<Box<[u8]>>> = None;
pub static mut ERROR_QUEUE: Option<VecDeque<String>> = None;
pub static mut SERVER_ADDR: ServerAddr = ServerAddr::Finding;

// Javascript methods
extern "C" {
    pub fn naia_connect(server_socket_address: JsObject, rtc_path: JsObject);
    pub fn naia_send(message: JsObject);
    pub fn naia_free_object(js_object: JsObjectWeak);
    pub fn naia_create_string(buf: *const u8, max_len: u32) -> JsObject;
    pub fn naia_unwrap_to_str(js_object: JsObjectWeak, buf: *mut u8, max_len: u32);
    pub fn naia_string_length(js_object: JsObjectWeak) -> u32;
    pub fn naia_create_u8_array(buf: *const u8, max_len: u32) -> JsObject;
    pub fn naia_unwrap_to_u8_array(js_object: JsObjectWeak, buf: *mut u8, max_len: u32);
    pub fn naia_u8_array_length(js_object: JsObjectWeak) -> u32;
}

// Rust methods
#[no_mangle]
pub extern "C" fn receive(message: JsObject) {
    let mut message_string = Vec::<u8>::new();

    message.to_u8_array(&mut message_string);

    unsafe {
        if let Some(msg_queue) = &mut MESSAGE_QUEUE {
            msg_queue.push_back(message_string.into_boxed_slice());
        }
    }
}

#[no_mangle]
pub extern "C" fn error(error: JsObject) {
    let mut error_string = String::new();

    error.to_string(&mut error_string);

    unsafe {
        if let Some(error_queue) = &mut ERROR_QUEUE {
            error_queue.push_back(error_string);
        }
    }
}

#[no_mangle]
pub extern "C" fn receive_candidate(candidate_js: JsObject) {
    let mut candidate_str = String::new();

    candidate_js.to_string(&mut candidate_str);

    unsafe {
        SERVER_ADDR = candidate_to_addr(&candidate_str);
    }
}

// JsObject
#[repr(transparent)]
pub struct JsObject(u32);

impl JsObject {
    pub fn weak(&self) -> JsObjectWeak {
        JsObjectWeak(self.0)
    }
}
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct JsObjectWeak(u32);

impl Drop for JsObject {
    fn drop(&mut self) {
        unsafe {
            naia_free_object(self.weak());
        }
    }
}

impl JsObject {
    pub fn string(string: &str) -> JsObject {
        unsafe { naia_create_string(string.as_ptr() as _, string.len() as _) }
    }

    pub fn to_string(&self, buf: &mut String) {
        let len = unsafe { naia_string_length(self.weak()) };

        if len as usize > buf.len() {
            buf.reserve(len as usize - buf.len());
        }
        unsafe { buf.as_mut_vec().set_len(len as usize) };
        unsafe { naia_unwrap_to_str(self.weak(), buf.as_mut_vec().as_mut_ptr(), len as u32) };
    }

    pub fn to_u8_array(&self, buf: &mut Vec<u8>) {
        let len = unsafe { naia_u8_array_length(self.weak()) };

        if len as usize > buf.len() {
            buf.reserve(len as usize - buf.len());
        }
        unsafe { buf.set_len(len as usize) };
        unsafe { naia_unwrap_to_u8_array(self.weak(), buf.as_mut_ptr(), len as u32) };
    }
}
