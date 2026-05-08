use std::{error::Error, fmt};

/// Errors that can be returned by the naia client.
///
/// Returned by methods such as [`Client::send_message`] and the
/// packet-processing loop when an unrecoverable transport or protocol
/// condition is encountered.
///
/// [`Client::send_message`]: crate::Client::send_message
#[derive(Debug)]
pub enum NaiaClientError {
    /// A general descriptive error message.
    Message(String),
    /// An error from an underlying layer, boxed for type erasure.
    Wrapped(Box<dyn Error + Send>),
    /// A packet could not be sent to the server.
    SendError,
    /// A packet could not be read from the socket.
    RecvError,
    /// A numeric entity or message identifier was malformed or out of range.
    IdError(u16),
}

impl NaiaClientError {
    pub fn from_message(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl fmt::Display for NaiaClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Message(msg) => write!(f, "Naia Client Error: {}", msg),
            Self::Wrapped(boxed_err) => fmt::Display::fmt(boxed_err.as_ref(), f),
            Self::SendError => write!(f, "Naia Client Error: Send Error"),
            Self::RecvError => write!(f, "Naia Client Error: Recv Error"),
            Self::IdError(code) => write!(f, "Naia Client Error: Id Error: {}", code),
        }
    }
}

impl Error for NaiaClientError {}
unsafe impl Send for NaiaClientError {}
unsafe impl Sync for NaiaClientError {}
