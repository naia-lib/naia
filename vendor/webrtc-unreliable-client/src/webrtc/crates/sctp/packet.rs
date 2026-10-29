use crate::webrtc::sctp::chunk::Chunk;

use crate::webrtc::sctp::chunk::chunk_abort::ChunkAbort;
use crate::webrtc::sctp::chunk::chunk_cookie_ack::ChunkCookieAck;
use crate::webrtc::sctp::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::webrtc::sctp::chunk::chunk_error::ChunkError;
use crate::webrtc::sctp::chunk::chunk_forward_tsn::ChunkForwardTsn;
use crate::webrtc::sctp::chunk::chunk_header::*;
use crate::webrtc::sctp::chunk::chunk_heartbeat::ChunkHeartbeat;
use crate::webrtc::sctp::chunk::chunk_init::ChunkInit;
use crate::webrtc::sctp::chunk::chunk_payload_data::ChunkPayloadData;
use crate::webrtc::sctp::chunk::chunk_reconfig::ChunkReconfig;
use crate::webrtc::sctp::chunk::chunk_selective_ack::ChunkSelectiveAck;
use crate::webrtc::sctp::chunk::chunk_shutdown::ChunkShutdown;
use crate::webrtc::sctp::chunk::chunk_shutdown_ack::ChunkShutdownAck;
use crate::webrtc::sctp::chunk::chunk_shutdown_complete::ChunkShutdownComplete;
use crate::webrtc::sctp::chunk::chunk_type::*;
use crate::webrtc::sctp::error::{Error, Result};
use crate::webrtc::sctp::util::*;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use crc::{Crc, CRC_32_ISCSI};
use std::fmt;

///Packet represents an SCTP packet, defined in https://tools.ietf.org/html/rfc4960#section-3
///An SCTP packet is composed of a common header and chunks.  A chunk
///contains either control information or user data.
///
///
///SCTP Packet Format
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                        Common Header                          |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                          Chunk #1                             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                           ...                                 |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                          Chunk #n                             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
///
///SCTP Common Header Format
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Source Value Number        |     Destination Value Number |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                      Verification Tag                         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                           Checksum                            |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug)]
pub(crate) struct Packet {
    pub(crate) source_port: u16,
    pub(crate) destination_port: u16,
    pub(crate) verification_tag: u32,
    pub(crate) chunks: Vec<Box<dyn Chunk + Send + Sync>>,
}

/// makes packet printable
impl fmt::Display for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = format!(
            "Packet:
        source_port: {}
        destination_port: {}
        verification_tag: {}
        ",
            self.source_port, self.destination_port, self.verification_tag,
        );
        for chunk in &self.chunks {
            res += format!("Chunk: {}", chunk).as_str();
        }
        write!(f, "{}", res)
    }
}

pub(crate) const PACKET_HEADER_SIZE: usize = 12;

impl Packet {
    pub(crate) fn unmarshal(raw: &Bytes) -> Result<Self> {
        if raw.len() < PACKET_HEADER_SIZE {
            return Err(Error::ErrPacketRawTooSmall);
        }

        let reader = &mut raw.clone();

        let source_port = reader.get_u16();
        let destination_port = reader.get_u16();
        let verification_tag = reader.get_u32();
        let their_checksum = reader.get_u32_le();
        let our_checksum = generate_packet_checksum(raw);

        if their_checksum != our_checksum {
            return Err(Error::ErrChecksumMismatch);
        }

        let mut chunks = vec![];
        let mut offset = PACKET_HEADER_SIZE;
        loop {
            // Exact match, no more chunks
            if offset == raw.len() {
                break;
            } else if offset + CHUNK_HEADER_SIZE > raw.len() {
                return Err(Error::ErrParseSctpChunkNotEnoughData);
            }

            let ct = ChunkType(raw[offset]);
            let c: Box<dyn Chunk + Send + Sync> = match ct {
                CT_INIT => Box::new(ChunkInit::unmarshal(&raw.slice(offset..))?),
                CT_INIT_ACK => Box::new(ChunkInit::unmarshal(&raw.slice(offset..))?),
                CT_ABORT => Box::new(ChunkAbort::unmarshal(&raw.slice(offset..))?),
                CT_COOKIE_ECHO => Box::new(ChunkCookieEcho::unmarshal(&raw.slice(offset..))?),
                CT_COOKIE_ACK => Box::new(ChunkCookieAck::unmarshal(&raw.slice(offset..))?),
                CT_HEARTBEAT => Box::new(ChunkHeartbeat::unmarshal(&raw.slice(offset..))?),
                CT_PAYLOAD_DATA => Box::new(ChunkPayloadData::unmarshal(&raw.slice(offset..))?),
                CT_SACK => Box::new(ChunkSelectiveAck::unmarshal(&raw.slice(offset..))?),
                CT_RECONFIG => Box::new(ChunkReconfig::unmarshal(&raw.slice(offset..))?),
                CT_FORWARD_TSN => Box::new(ChunkForwardTsn::unmarshal(&raw.slice(offset..))?),
                CT_ERROR => Box::new(ChunkError::unmarshal(&raw.slice(offset..))?),
                CT_SHUTDOWN => Box::new(ChunkShutdown::unmarshal(&raw.slice(offset..))?),
                CT_SHUTDOWN_ACK => Box::new(ChunkShutdownAck::unmarshal(&raw.slice(offset..))?),
                CT_SHUTDOWN_COMPLETE => {
                    Box::new(ChunkShutdownComplete::unmarshal(&raw.slice(offset..))?)
                }
                _ => return Err(Error::ErrUnmarshalUnknownChunkType),
            };

            let chunk_value_padding = get_padding_size(c.value_length());
            offset += CHUNK_HEADER_SIZE + c.value_length() + chunk_value_padding;
            chunks.push(c);
        }

        Ok(Packet {
            source_port,
            destination_port,
            verification_tag,
            chunks,
        })
    }

    pub(crate) fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        // Populate static headers
        // 8-12 is Checksum which will be populated when packet is complete
        writer.put_u16(self.source_port);
        writer.put_u16(self.destination_port);
        writer.put_u32(self.verification_tag);

        // Populate chunks
        let mut raw = BytesMut::new();
        for c in &self.chunks {
            let chunk_raw = c.marshal()?;
            raw.extend(chunk_raw);

            let padding_needed = get_padding_size(raw.len());
            if padding_needed != 0 {
                raw.extend(vec![0u8; padding_needed]);
            }
        }
        let raw = raw.freeze();

        let hasher = Crc::<u32>::new(&CRC_32_ISCSI);
        let mut digest = hasher.digest();
        digest.update(writer);
        digest.update(&FOUR_ZEROES);
        digest.update(&raw[..]);
        let checksum = digest.finalize();

        // Checksum is already in BigEndian
        // Using LittleEndian stops it from being flipped
        writer.put_u32_le(checksum);
        writer.extend(raw);

        Ok(writer.len())
    }

    pub(crate) fn marshal(&self) -> Result<Bytes> {
        let mut buf = BytesMut::with_capacity(PACKET_HEADER_SIZE);
        self.marshal_to(&mut buf)?;
        Ok(buf.freeze())
    }
}

impl Packet {
    pub(crate) fn check_packet(&self) -> Result<()> {
        // All packets must adhere to these rules

        // This is the SCTP sender's port number.  It can be used by the
        // receiver in combination with the source IP address, the SCTP
        // destination port, and possibly the destination IP address to
        // identify the association to which this packet belongs.  The port
        // number 0 MUST NOT be used.
        if self.source_port == 0 {
            return Err(Error::ErrSctpPacketSourcePortZero);
        }

        // This is the SCTP port number to which this packet is destined.
        // The receiving host will use this port number to de-multiplex the
        // SCTP packet to the correct receiving endpoint/application.  The
        // port number 0 MUST NOT be used.
        if self.destination_port == 0 {
            return Err(Error::ErrSctpPacketDestinationPortZero);
        }

        // Check values on the packet that are specific to a particular chunk type
        for c in &self.chunks {
            if let Some(ci) = c.as_any().downcast_ref::<ChunkInit>() {
                if !ci.is_ack {
                    // An INIT or INIT ACK chunk MUST NOT be bundled with any other chunk.
                    // They MUST be the only chunks present in the SCTP packets that carry
                    // them.
                    if self.chunks.len() != 1 {
                        return Err(Error::ErrInitChunkBundled);
                    }

                    // A packet containing an INIT chunk MUST have a zero Verification
                    // Tag.
                    if self.verification_tag != 0 {
                        return Err(Error::ErrInitChunkVerifyTagNotZero);
                    }
                }
            }
        }

        Ok(())
    }
}
