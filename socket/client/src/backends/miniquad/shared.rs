#[no_mangle]
pub extern "C" fn naia_socket_crate_version() -> u32 {
    let major = dbg!(env!("CARGO_PKG_VERSION_MAJOR").parse::<u32>().unwrap());
    let minor = env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap();
    let patch = dbg!(env!("CARGO_PKG_VERSION_PATCH").parse::<u32>().unwrap());

    (major << 24) + (minor << 16) + patch
}

use naia_socket_shared::IdentityToken;

use crate::{
    socket_table::{SocketId, SocketTable},
    wasm_utils::candidate_to_addr,
};

// The live sockets' state, keyed by id. The `#[no_mangle] extern "C"`
// callbacks below must stay process-global entry points -- the JS bridge
// calls them by name -- so they route through this one table by the socket
// id the bridge echoes on every call. Each `connect` opens a fresh slot;
// opening a second socket never resets the first (naia-lib/naia#193).
pub static mut SOCKET_TABLE: Option<SocketTable> = None;

/// Opens a fresh per-socket slot and returns its id, to be passed as the
/// first argument of `naia_connect` so the JS bridge files its connection
/// record -- and every later callback -- under the same id.
pub fn alloc_socket() -> u32 {
    // Safety: SOCKET_TABLE is written here and subsequently only accessed
    // from the same wasm32 thread via the JS bridge callbacks and the
    // socket handles. wasm32 is single-threaded, so this is safe.
    unsafe {
        let table = SOCKET_TABLE.get_or_insert_with(SocketTable::new);
        return table.connect().0;
    }
}

/// Closes a socket's slot, dropping its queued state.
pub fn free_socket(socket_id: u32) {
    // Safety: see alloc_socket above.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            table.disconnect(SocketId(socket_id));
        }
    }
}

// Javascript methods
extern "C" {
    pub fn naia_is_connected(socket_id: u32) -> bool;
    pub fn naia_connect(
        socket_id: u32,
        server_socket_address: JsObject,
        rtc_path: JsObject,
        auth_str: JsObject,
        protocol_id: JsObject,
    );
    pub fn naia_disconnect(socket_id: u32);
    pub fn naia_send(socket_id: u32, message: JsObject) -> bool;
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
pub extern "C" fn receive_id(socket_id: u32, id_token: JsObject) {
    let mut id_token_string = String::new();

    id_token.to_string(&mut id_token_string);

    // Safety: SOCKET_TABLE is a static mut acting as the single-producer /
    // single-consumer store between the JS bridge callbacks (producer) and
    // the Rust game loop (consumer). wasm32 is single-threaded — the JS event
    // loop and Rust code never execute concurrently, so accessing it without
    // synchronization is safe on this target. None of the callback functions
    // re-enter. A callback for an unknown (disconnected) socket is ignored.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            if let Some(state) = table.get_mut(SocketId(socket_id)) {
                if let Some(id_cell) = &mut state.id_cell {
                    *id_cell = IdentityToken::from_signaling_string(&id_token_string);
                }
            }
        }
    }
}

/// Records a non-200 signaling answer for the identity path: the HTTP status
/// code with the raw response body, which on a rejection carries the optional
/// base64-encoded reason message. Called by `naia_socket.js` only when the
/// session POST completes with a non-200 status; network-level failures (no
/// status at all) keep going to `error`/`ERROR_QUEUE` as generic errors.
#[no_mangle]
pub extern "C" fn receive_auth_error(socket_id: u32, status: JsObject, body: JsObject) {
    let mut status_string = String::new();
    let mut body_string = String::new();

    status.to_string(&mut status_string);
    body.to_string(&mut body_string);

    // Safety: see receive_id above.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            if let Some(state) = table.get_mut(SocketId(socket_id)) {
                if let Some(auth_error_cell) = &mut state.auth_error_cell {
                    if let Ok(status_code) = status_string.trim().parse::<u16>() {
                        *auth_error_cell = Some((status_code, body_string));
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn receive(socket_id: u32, message: JsObject) {
    let mut message_string = Vec::<u8>::new();

    message.to_u8_array(&mut message_string);

    // Safety: see receive_id above.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            if let Some(state) = table.get_mut(SocketId(socket_id)) {
                state
                    .message_queue
                    .push_back(message_string.into_boxed_slice());
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn error(socket_id: u32, error: JsObject) {
    let mut error_string = String::new();

    error.to_string(&mut error_string);

    // Safety: see receive_id above.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            if let Some(state) = table.get_mut(SocketId(socket_id)) {
                state.error_queue.push_back(error_string);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn receive_candidate(socket_id: u32, candidate_js: JsObject) {
    let mut candidate_str = String::new();

    candidate_js.to_string(&mut candidate_str);

    // Safety: see receive_id above.
    unsafe {
        if let Some(table) = &mut SOCKET_TABLE {
            if let Some(state) = table.get_mut(SocketId(socket_id)) {
                state.server_addr = candidate_to_addr(&candidate_str);
            }
        }
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
        // Safety: self.0 is a valid JS object handle issued by the JS bridge. Calling
        // naia_free_object once on drop is the correct ownership protocol for JsObject.
        unsafe {
            naia_free_object(self.weak());
        }
    }
}

impl JsObject {
    pub fn string(string: &str) -> JsObject {
        // Safety: string.as_ptr() is valid for string.len() bytes for the duration of this call.
        // naia_create_string copies the bytes into a JS string and returns a new handle.
        unsafe { naia_create_string(string.as_ptr() as _, string.len() as _) }
    }

    pub fn to_string(&self, buf: &mut String) {
        // Safety: naia_string_length returns the byte length of the JS string; we reserve at
        // least that many bytes in buf before calling naia_unwrap_to_str. set_len is sound
        // because naia_unwrap_to_str writes exactly len valid UTF-8 bytes into the buffer.
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
