use thiserror::Error;

use crate::webrtc::util::KeyingMaterialExporterError;
use rcgen::Error as RcgenError;
use smol::channel::SendError as MpscSendError;
use std::io;
use std::string::FromUtf8Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    #[error("conn is closed")]
    ErrConnClosed,
    #[error("read/write timeout")]
    ErrDeadlineExceeded,
    #[error("buffer is too small")]
    ErrBufferTooSmall,
    #[error("handshake is in progress")]
    ErrHandshakeInProgress,
    #[error("invalid content type")]
    ErrInvalidContentType,
    #[error("packet length and declared length do not match")]
    ErrInvalidPacketLength,
    #[error("client+server do not support any shared cipher suites")]
    ErrCipherSuiteNoIntersection,
    #[error("client required Extended Master Secret extension, but server does not support it")]
    ErrClientRequiredButNoServerEms,
    #[error("cookie must not be longer then 255 bytes")]
    ErrCookieTooLong,
    #[error("cipher spec invalid")]
    ErrInvalidCipherSpec,
    #[error("invalid or unknown cipher suite")]
    ErrInvalidCipherSuite,
    #[error("unable to determine if ClientKeyExchange is a public key or PSK Identity")]
    ErrInvalidClientKeyExchange,
    #[error("invalid extension type")]
    ErrInvalidExtensionType,
    #[error("invalid named curve")]
    ErrInvalidNamedCurve,
    #[error("invalid private key type")]
    ErrInvalidPrivateKey,
    #[error("named curve and private key type does not match")]
    ErrNamedCurveAndPrivateKeyMismatch,
    #[error("invalid server name format")]
    ErrInvalidSniFormat,
    #[error("connection can not be created, no CipherSuites satisfy this Config")]
    ErrNoAvailableCipherSuites,
    #[error("connection can not be created, no SignatureScheme satisfy this Config")]
    ErrNoAvailableSignatureSchemes,
    #[error("unsupported protocol version")]
    ErrUnsupportedProtocolVersion,
    #[error("expected and actual verify data does not match")]
    ErrVerifyDataMismatch,
    #[error("unable to verify key signature, unimplemented")]
    ErrKeySignatureVerifyUnimplemented,
    #[error("data length and declared length do not match")]
    ErrLengthMismatch,
    #[error("buffer not long enough to contain nonce")]
    ErrNotEnoughRoomForNonce,
    #[error("feature has not been implemented yet")]
    ErrNotImplemented,
    #[error("sequence number overflow")]
    ErrSequenceNumberOverflow,
    #[error("ApplicationData with epoch of 0")]
    ErrApplicationDataEpochZero,
    #[error("unhandled contentType")]
    ErrUnhandledContextType,
    #[error("empty fragment")]
    ErrEmptyFragment,
    #[error("Alert is Fatal or Close Notify")]
    ErrAlertFatalOrClose,

    #[error("{0}")]
    Io(#[source] IoError),
    #[error("{0}")]
    Util(#[from] crate::webrtc::util::Error),
    #[error("utf8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    P256(#[source] P256Error),
    #[error("{0}")]
    RcGen(#[from] RcgenError),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("keying material: {0}")]
    KeyingMaterial(#[from] KeyingMaterialExporterError),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    Other(String),
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

#[derive(Debug, Error)]
#[error("{0}")]
pub(crate) struct P256Error(#[source] p256::elliptic_curve::Error);

impl PartialEq for P256Error {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl From<p256::elliptic_curve::Error> for Error {
    fn from(e: p256::elliptic_curve::Error) -> Self {
        Error::P256(P256Error(e))
    }
}

// Because channel SendError is parameterized, we sadly lose the backtrace.
impl<T> From<MpscSendError<T>> for Error {
    fn from(e: MpscSendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}
