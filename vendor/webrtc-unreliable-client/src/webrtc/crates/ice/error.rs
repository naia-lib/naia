use thiserror::Error;

use std::io;
use std::net;
use std::num::ParseIntError;
use std::time::SystemTimeError;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    /// Indicates local username fragment insufficient bits are provided.
    /// Have to be at least 24 bits long.
    #[error("local username fragment is less than 24 bits long")]
    ErrLocalUfragInsufficientBits,

    /// Indicates local passoword insufficient bits are provided.
    /// Have to be at least 128 bits long.
    #[error("local password is less than 128 bits long")]
    ErrLocalPwdInsufficientBits,

    /// Indicates the agent is closed.
    #[error("the agent is closed")]
    ErrClosed,

    /// Indicates agent connection was canceled by the caller.
    #[error("connecting canceled by caller")]
    ErrCanceledByCaller,

    /// Indicates agent was started twice.
    #[error("attempted to start agent twice")]
    ErrMultipleStart,

    /// Indicates agent was started with an empty remote ufrag.
    #[error("remote ufrag is empty")]
    ErrRemoteUfragEmpty,

    /// Indicates agent was started with an empty remote pwd.
    #[error("remote pwd is empty")]
    ErrRemotePwdEmpty,

    /// Indicates agent was started without on_candidate.
    #[error("no on_candidate provided")]
    ErrNoOnCandidateHandler,

    /// Indicates GatherCandidates has been called multiple times.
    #[error("attempting to gather candidates during gathering state")]
    ErrMultipleGatherAttempted,

    /// Indicates we were unable to parse a candidate address.
    #[error("failed to parse address")]
    ErrAddressParseFailed,

    /// Indicates that non host candidates were selected for a lite agent.
    #[error("lite agents must only use host candidates")]
    ErrLiteUsingNonHostCandidates,

    /// Indicates that the specified NAT1To1IPCandidateType is unsupported.
    #[error("unsupported 1:1 NAT IP candidate type")]
    ErrUnsupportedNat1to1IpCandidateType,

    /// Indicates that the given 1:1 NAT IP mapping is invalid.
    #[error("invalid 1:1 NAT IP mapping")]
    ErrInvalidNat1to1IpMapping,

    /// IPNotFound in NAT1To1IPMapping.
    #[error("external mapped IP not found")]
    ErrExternalMappedIpNotFound,

    /// Indicates that the mDNS gathering cannot be used along with 1:1 NAT IP mapping for host
    /// candidate.
    #[error("mDNS gathering cannot be used with 1:1 NAT IP mapping for host candidate")]
    ErrMulticastDnsWithNat1to1IpMapping,

    /// Indicates that 1:1 NAT IP mapping for host candidate is requested, but the host candidate
    /// type is disabled.
    #[error("1:1 NAT IP mapping for host candidate ineffective")]
    ErrIneffectiveNat1to1IpMappingHost,

    /// Indicates an invalid MulticastDNSHostName.
    #[error("invalid mDNS HostName, must end with .local and can only contain a single '.'")]
    ErrInvalidMulticastDnshostName,

    /// Indicates Restart was called when Agent is in GatheringStateGathering.
    #[error("ICE Agent can not be restarted when gathering")]
    ErrRestartWhenGathering,

    #[error("attribute not long enough to be ICE candidate")]
    ErrAttributeTooShortIceCandidate,
    #[error("could not parse related addresses")]
    ErrParseRelatedAddr,
    #[error("unknown candidate type")]
    ErrUnknownCandidateType,
    #[error("unable to determine networkType")]
    ErrDetermineNetworkType,
    #[error("username mismatch")]
    ErrMismatchUsername,

    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("parse addr: {0}")]
    ParseIp(#[from] net::AddrParseError),
    #[error("{0}")]
    Io(#[source] IoError),
    #[error("{0}")]
    Util(#[from] crate::webrtc::util::Error),
    #[error("{0}")]
    Stun(#[from] crate::webrtc::stun::Error),

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

impl From<SystemTimeError> for Error {
    fn from(e: SystemTimeError) -> Self {
        Error::Other(e.to_string())
    }
}
