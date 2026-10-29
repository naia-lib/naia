use thiserror::Error;

use std::io;
use std::num::ParseIntError;
use std::string::FromUtf8Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("SdpInvalidSyntax: {0}")]
    SdpInvalidSyntax(String),
    #[error("SdpInvalidValue: {0}")]
    SdpInvalidValue(String),
    #[error("sdp: empty time_descriptions")]
    SdpEmptyTimeDescription,
    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),
}

#[derive(Debug, Error)]
#[error("io error: {0}")]
pub(crate) struct IoError(#[from] pub(crate) io::Error);

// Workaround for wanting PartialEq for io::Error.
impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(IoError(e))
    }
}
