use std::{error::Error, fmt};

#[derive(Debug)]
pub enum NaiaServerError {
    Wrapped(Box<dyn Error>),
}

impl fmt::Display for NaiaServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            NaiaServerError::Wrapped(boxed_err) => fmt::Display::fmt(boxed_err.as_ref(), f),
        }
    }
}

impl Error for NaiaServerError {}
unsafe impl Send for NaiaServerError {}
unsafe impl Sync for NaiaServerError {}
