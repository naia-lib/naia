#![recursion_limit = "256"]

#[macro_use]
extern crate lazy_static;

mod socket;

pub use socket::{SessionError, Socket, SocketIo, MAX_MESSAGE_SIZE};

mod webrtc;
