use thiserror::Error;

use smol::channel::SendError as MpscSendError;
use std::io;
use std::string::FromUtf8Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    #[error("attribute not found")]
    ErrAttributeNotFound,
    #[error("unexpected EOF")]
    ErrUnexpectedEof,
    #[error("attribute size is invalid")]
    ErrAttributeSizeInvalid,
    #[error("attribute size overflow")]
    ErrAttributeSizeOverflow,
    #[error("unexpected EOF: not enough bytes to read header")]
    ErrUnexpectedHeaderEof,
    #[error("integrity check failed")]
    ErrIntegrityMismatch,
    #[error("FINGERPRINT before MESSAGE-INTEGRITY attribute")]
    ErrFingerprintBeforeIntegrity,
    #[error("{0}")]
    Other(String),
    #[error("utf8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("{0}")]
    Util(#[from] crate::webrtc::util::Error),
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

// Because channel SendError is parameterized, we sadly lose the backtrace.
impl<T> From<MpscSendError<T>> for Error {
    fn from(e: MpscSendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}
