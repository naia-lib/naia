use futures_channel::mpsc::SendError as MpscSendError;
use rcgen::Error as RcgenError;
use std::future::Future;
use std::num::ParseIntError;
use std::pin::Pin;
use std::string::FromUtf8Error;
use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    /// ErrConnectionClosed indicates an operation executed after connection
    /// has already been closed.
    #[error("connection closed")]
    ErrConnectionClosed,

    /// ErrNonCertificate indicates that there is no certificate
    #[error("no certificate")]
    ErrNonCertificate,

    /// ErrNoRemoteDescription indicates that an operation was rejected because
    /// the remote description is not set
    #[error("remote description is not set")]
    ErrNoRemoteDescription,

    /// ErrSessionDescriptionNoFingerprint indicates set_remote_description was called with a SessionDescription that has no
    /// fingerprint
    #[error("set_remote_description called with no fingerprint")]
    ErrSessionDescriptionNoFingerprint,

    /// ErrSessionDescriptionInvalidFingerprint indicates set_remote_description was called with a SessionDescription that
    /// has an invalid fingerprint
    #[error("set_remote_description called with an invalid fingerprint")]
    ErrSessionDescriptionInvalidFingerprint,

    /// ErrSessionDescriptionConflictingFingerprints indicates set_remote_description was called with a SessionDescription that
    /// has an conflicting fingerprints
    #[error("set_remote_description called with multiple conflicting fingerprint")]
    ErrSessionDescriptionConflictingFingerprints,

    /// ErrSessionDescriptionMissingIceUfrag indicates set_remote_description was called with a SessionDescription that
    /// is missing an ice-ufrag value
    #[error("set_remote_description called with no ice-ufrag")]
    ErrSessionDescriptionMissingIceUfrag,

    /// ErrSessionDescriptionMissingIcePwd indicates set_remote_description was called with a SessionDescription that
    /// is missing an ice-pwd value
    #[error("set_remote_description called with no ice-pwd")]
    ErrSessionDescriptionMissingIcePwd,

    /// ErrSessionDescriptionConflictingIceUfrag  indicates set_remote_description was called with a SessionDescription that
    /// contains multiple conflicting ice-ufrag values
    #[error("set_remote_description called with multiple conflicting ice-ufrag values")]
    ErrSessionDescriptionConflictingIceUfrag,

    /// ErrSessionDescriptionConflictingIcePwd indicates set_remote_description was called with a SessionDescription that
    /// contains multiple conflicting ice-pwd values
    #[error("set_remote_description called with multiple conflicting ice-pwd values")]
    ErrSessionDescriptionConflictingIcePwd,

    #[error("datachannel not opened yet, try calling Detach from OnOpen")]
    ErrDetachBeforeOpened,
    #[error("attempted to start DTLSTransport that is not in new state")]
    ErrInvalidDTLSStart,
    #[error("identity provider is not implemented")]
    ErrIdentityProviderNotImplemented,
    #[error("ICE connection not started")]
    ErrICEConnectionNotStarted,
    #[error("unknown candidate type")]
    ErrICECandidateTypeUnknown,
    #[error("ICEAgent does not exist")]
    ErrICEAgentNotExist,
    #[error("unknown ICE Role")]
    ErrICERoleUnknown,
    #[error("new sdp does not match previous offer")]
    ErrSDPDoesNotMatchOffer,
    #[error("new sdp does not match previous answer")]
    ErrSDPDoesNotMatchAnswer,
    #[error("provided value is not a valid enum value of type SDPType")]
    ErrPeerConnSDPTypeInvalidValue,
    #[error("invalid state change op")]
    ErrPeerConnStateChangeInvalid,
    #[error("invalid SDP type supplied to SetLocalDescription()")]
    ErrPeerConnSDPTypeInvalidValueSetLocalDescription,
    #[error("DTLS not established")]
    ErrSCTPTransportDTLS,
    #[error("can't rollback from stable state")]
    ErrSignalingStateCannotRollback,
    #[error("invalid proposed signaling state transition")]
    ErrSignalingStateProposedTransitionInvalid,
    #[error("ICETransport can only be called in ICETransportStateNew")]
    ErrICETransportNotInNew,
    #[error("SCTP is not established")]
    ErrSCTPNotEstablished,

    #[error("{0}")]
    Util(#[from] crate::webrtc::util::Error),
    #[error("{0}")]
    Ice(#[from] crate::webrtc::ice::Error),
    #[error("{0}")]
    Dtls(#[from] crate::webrtc::dtls::Error),
    #[error("{0}")]
    Data(#[from] crate::webrtc::internal::Error),
    #[error("{0}")]
    Sctp(#[from] crate::webrtc::sctp::Error),
    #[error("{0}")]
    Sdp(#[from] crate::webrtc::sdp::Error),

    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    RcGen(#[from] RcgenError),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

pub(crate) type OnErrorHdlrFn =
    Box<dyn (FnMut(Error) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

// futures-channel's mpsc SendError carries no payload; keep the message only.
impl From<MpscSendError> for Error {
    fn from(e: MpscSendError) -> Self {
        Error::MpscSend(e.to_string())
    }
}

// `smol::channel` (async-channel) replaces `tokio::sync::mpsc` for all
// intra-crate signaling; both its send errors funnel into the same variant.
impl<T> From<smol::channel::SendError<T>> for Error {
    fn from(e: smol::channel::SendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}

impl<T> From<smol::channel::TrySendError<T>> for Error {
    fn from(e: smol::channel::TrySendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}

impl PartialEq<crate::webrtc::ice::Error> for Error {
    fn eq(&self, other: &crate::webrtc::ice::Error) -> bool {
        if let Error::Ice(e) = self {
            return e == other;
        }
        false
    }
}
