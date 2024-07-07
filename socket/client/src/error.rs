use std::{error::Error, fmt};

/// An Error type specifically related to the Naia Client Socket
/// This is under construction and needs to be cleaned up
#[derive(Debug)]
pub enum NaiaClientSocketError {
    /// A simple error message
    Message(String),
    /// An error indicating an inability to send to the given address
    SendError,
}

impl fmt::Display for NaiaClientSocketError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            NaiaClientSocketError::Message(msg) => write!(f, "Naia Client Socket Error: {}", msg),
            NaiaClientSocketError::SendError => write!(f, "Naia Client Socket Send Error"),
        }
    }
}

impl Error for NaiaClientSocketError {}
