
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum GaiaClientError {
    Message(String),
    Wrapped(Box<dyn Error + Send>)
}

impl fmt::Display for GaiaClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            GaiaClientError::Message(msg) => write!(f, "Gaia Client Error: {}", msg),
            GaiaClientError::Wrapped(boxed_err) => fmt::Display::fmt(boxed_err.as_ref(), f),
        }
    }
}

impl Error for GaiaClientError {}