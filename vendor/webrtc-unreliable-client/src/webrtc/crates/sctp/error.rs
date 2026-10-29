use std::io;
use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Error {
    #[error("raw is too small for a SCTP chunk")]
    ErrChunkHeaderTooSmall,
    #[error("not enough data left in SCTP packet to satisfy requested length")]
    ErrChunkHeaderNotEnoughSpace,
    #[error("chunk PADDING is non-zero at offset")]
    ErrChunkHeaderPaddingNonZero,
    #[error("chunk has invalid length")]
    ErrChunkHeaderInvalidLength,

    #[error("ChunkType is not of type ABORT")]
    ErrChunkTypeNotAbort,
    #[error("ChunkType is not of type COOKIEACK")]
    ErrChunkTypeNotCookieAck,
    #[error("ChunkType is not of type COOKIEECHO")]
    ErrChunkTypeNotCookieEcho,
    #[error("ChunkType is not of type ctError")]
    ErrChunkTypeNotCtError,
    #[error("chunk too short")]
    ErrChunkTooShort,
    #[error("ChunkType is not of type ForwardTsn")]
    ErrChunkTypeNotForwardTsn,
    #[error("ChunkType is not of type HEARTBEAT")]
    ErrChunkTypeNotHeartbeat,
    #[error("ChunkType is not of type HEARTBEATACK")]
    ErrChunkTypeNotHeartbeatAck,
    #[error("heartbeat is not long enough to contain Heartbeat Info")]
    ErrHeartbeatNotLongEnoughInfo,
    #[error("heartbeat should only have HEARTBEAT param")]
    ErrHeartbeatParam,
    #[error("heartbeat Ack must have one param")]
    ErrHeartbeatAckParams,
    #[error("heartbeat Ack must have one param, and it should be a HeartbeatInfo")]
    ErrHeartbeatAckNotHeartbeatInfo,

    #[error("raw is too small for error cause")]
    ErrErrorCauseTooSmall,

    #[error("unhandled ParamType")]
    ErrParamTypeUnhandled,

    #[error("param header too short")]
    ErrParamHeaderTooShort,

    #[error("outgoing SSN reset request parameter too short")]
    ErrSsnResetRequestParamTooShort,
    #[error("reconfig response parameter too short")]
    ErrReconfigRespParamTooShort,
    #[error("ChunkType is not of type INIT")]
    ErrChunkTypeNotTypeInit,
    #[error("chunk Value isn't long enough for mandatory parameters exp")]
    ErrChunkValueNotLongEnough,
    #[error("ChunkType of type INIT flags must be all 0")]
    ErrChunkTypeInitFlagZero,
    #[error("ChunkType of type INIT ACK InitiateTag must not be 0")]
    ErrChunkTypeInitInitateTagZero,
    #[error("INIT ACK inbound stream request must be > 0")]
    ErrInitInboundStreamRequestZero,
    #[error("INIT ACK outbound stream request must be > 0")]
    ErrInitOutboundStreamRequestZero,
    #[error("INIT ACK Advertised Receiver Window Credit (a_rwnd) must be >= 1500")]
    ErrInitAdvertisedReceiver1500,

    #[error("packet is smaller than the header size")]
    ErrChunkPayloadSmall,
    #[error("ChunkType is not of type PayloadData")]
    ErrChunkTypeNotPayloadData,
    #[error("ChunkType is not of type Reconfig")]
    ErrChunkTypeNotReconfig,
    #[error("ChunkReconfig has invalid ParamA")]
    ErrChunkReconfigInvalidParamA,

    #[error("ChunkType is not of type SACK")]
    ErrChunkTypeNotSack,
    #[error("SACK Chunk size is not large enough to contain header")]
    ErrSackSizeNotLargeEnoughInfo,

    #[error("invalid chunk size")]
    ErrInvalidChunkSize,
    #[error("ChunkType is not of type SHUTDOWN")]
    ErrChunkTypeNotShutdown,

    #[error("ChunkType is not of type SHUTDOWN-ACK")]
    ErrChunkTypeNotShutdownAck,
    #[error("ChunkType is not of type SHUTDOWN-COMPLETE")]
    ErrChunkTypeNotShutdownComplete,

    #[error("raw is smaller than the minimum length for a SCTP packet")]
    ErrPacketRawTooSmall,
    #[error("unable to parse SCTP chunk, not enough data for complete header")]
    ErrParseSctpChunkNotEnoughData,
    #[error("failed to unmarshal, contains unknown chunk type")]
    ErrUnmarshalUnknownChunkType,
    #[error("checksum mismatch theirs")]
    ErrChecksumMismatch,

    #[error("try again")]
    ErrTryAgain,

    #[error("abort chunk, with following errors")]
    ErrChunk,
    #[error("association handshake closed")]
    ErrAssociationHandshakeClosed,
    #[error("the init not stored to send")]
    ErrInitNotStoredToSend,
    #[error("cookieEcho not stored to send")]
    ErrCookieEchoNotStoredToSend,
    #[error("sctp packet must not have a source port of 0")]
    ErrSctpPacketSourcePortZero,
    #[error("sctp packet must not have a destination port of 0")]
    ErrSctpPacketDestinationPortZero,
    #[error("init chunk must not be bundled with any other chunk")]
    ErrInitChunkBundled,
    #[error("init chunk expects a verification tag of 0 on the packet when out-of-the-blue")]
    ErrInitChunkVerifyTagNotZero,
    #[error("todo: handle Init when in state")]
    ErrHandleInitState,
    #[error("no cookie in InitAck")]
    ErrInitAckNoCookie,
    #[error("there already exists a stream with identifier")]
    ErrStreamAlreadyExist,
    #[error("Failed to create a stream with identifier")]
    ErrStreamCreateFailed,
    #[error("unable to be popped from inflight queue TSN")]
    ErrInflightQueueTsnPop,
    #[error("requested non-existent TSN")]
    ErrTsnRequestNotExist,
    #[error("sending reset packet in non-Established state")]
    ErrResetPacketInStateNotExist,
    #[error("unexpected parameter type")]
    ErrParamterType,
    #[error("sending payload data in non-Established state")]
    ErrPayloadDataStateNotExist,
    #[error("unhandled chunk type")]
    ErrChunkTypeUnhandled,
    #[error("handshake failed (INIT ACK)")]
    ErrHandshakeInitAck,
    #[error("handshake failed (COOKIE ECHO)")]
    ErrHandshakeCookieEcho,

    #[error("outbound packet larger than maximum message size")]
    ErrOutboundPacketTooLarge,
    #[error("Stream closed")]
    ErrStreamClosed,
    #[error("Short buffer to be filled")]
    ErrShortBuffer,
    #[allow(dead_code)]
    #[error("Io EOF")]
    ErrEof,
    #[error("Invalid SystemTime")]
    ErrInvalidSystemTime,
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        match error {
            e @ Error::ErrEof => io::Error::new(io::ErrorKind::UnexpectedEof, e.to_string()),
            e @ Error::ErrStreamClosed => {
                io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string())
            }
            e => io::Error::other(e.to_string()),
        }
    }
}
