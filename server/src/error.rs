use std::{error::Error, fmt, net::SocketAddr};

#[derive(Debug)]
pub enum NaiaServerError {
    Message(String),
    Wrapped(Box<dyn Error>),
    SendError(SocketAddr),
    RecvError,
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
        }
    }
}

impl Error for NaiaServerError {}
unsafe impl Send for NaiaServerError {}
unsafe impl Sync for NaiaServerError {}
