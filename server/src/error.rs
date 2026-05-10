use std::{error::Error, fmt, net::SocketAddr};

/// Errors that can be returned by the naia server.
///
/// Returned by methods such as [`Server::send_message`] and the
/// packet-processing loop when an unrecoverable transport or protocol
/// condition is encountered.
///
/// [`Server::send_message`]: crate::Server::send_message
#[derive(Debug)]
pub enum NaiaServerError {
    /// A general descriptive error message.
    Message(String),
    /// An error from an underlying layer, boxed for type erasure.
    Wrapped(Box<dyn Error>),
    /// A packet could not be delivered to the given address.
    SendError(SocketAddr),
    /// A packet could not be read from the socket.
    RecvError,
    /// The supplied [`UserKey`] does not correspond to a currently connected
    /// user.
    ///
    /// [`UserKey`]: crate::UserKey
    UserNotFound,
}

impl NaiaServerError {
    pub fn from_message(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl fmt::Display for NaiaServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            NaiaServerError::Message(msg) => write!(f, "Naia Server Error: {}", msg),
            NaiaServerError::Wrapped(boxed_err) => fmt::Display::fmt(boxed_err.as_ref(), f),
            NaiaServerError::SendError(address) => {
                write!(f, "Naia Server Error: SendError: {}", address)
            }
            NaiaServerError::RecvError => {
                write!(f, "Naia Server Error: RecvError")
            }
            NaiaServerError::UserNotFound => {
                write!(f, "Naia Server Error: UserNotFound")
            }
        }
    }
}

impl Error for NaiaServerError {}
// Safety: NaiaServerError::Wrapped contains Box<dyn Error> which is !Send by default because
// the error object may not be Send. We assert Send here because naia's internal usage of the
// Wrapped variant only stores errors produced by transport layers that are themselves Send.
// Callers who construct Wrapped with a !Send error type violate this invariant.
unsafe impl Send for NaiaServerError {}
// Safety: Same reasoning as Send above — the contained error object must also be Sync.
unsafe impl Sync for NaiaServerError {}
