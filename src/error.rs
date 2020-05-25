use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum GaiaServerError {
    Wrapped(Box<dyn Error>)
}

impl fmt::Display for GaiaServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            GaiaServerError::Wrapped(boxed_err) => fmt::Display::fmt(boxed_err.as_ref(), f),
        }
    }
}

impl Error for GaiaServerError {}