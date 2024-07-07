use std::{error::Error, fmt};

#[derive(Debug)]
pub enum NaiaClientError {
    Message(String),
    Wrapped(Box<dyn Error + Send>),
    SendError,
    RecvError,
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
