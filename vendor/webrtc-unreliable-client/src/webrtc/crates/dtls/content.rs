use super::alert::*;
use super::application_data::*;
use super::change_cipher_spec::*;
use super::handshake::*;
use crate::webrtc::dtls::error::*;

use std::io::Write;

// https://tools.ietf.org/html/rfc4346#section-6.2.1
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub(crate) enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
    #[default]
    Invalid,
}

impl From<u8> for ContentType {
    fn from(val: u8) -> Self {
        match val {
            20 => ContentType::ChangeCipherSpec,
            21 => ContentType::Alert,
            22 => ContentType::Handshake,
            23 => ContentType::ApplicationData,
            _ => ContentType::Invalid,
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub(crate) enum Content {
    ChangeCipherSpec(ChangeCipherSpec),
    Alert(Alert),
    Handshake(Handshake),
    ApplicationData(ApplicationData),
}

impl Content {
    pub(crate) fn content_type(&self) -> ContentType {
        match self {
            Content::ChangeCipherSpec(c) => c.content_type(),
            Content::Alert(c) => c.content_type(),
            Content::Handshake(c) => c.content_type(),
            Content::ApplicationData(c) => c.content_type(),
        }
    }

    pub(crate) fn size(&self) -> usize {
        match self {
            Content::ChangeCipherSpec(c) => c.size(),
            Content::Alert(c) => c.size(),
            Content::Handshake(c) => c.size(),
            Content::ApplicationData(c) => c.size(),
        }
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Content::ChangeCipherSpec(c) => c.marshal(writer),
            Content::Alert(c) => c.marshal(writer),
            Content::Handshake(c) => c.marshal(writer),
            Content::ApplicationData(c) => c.marshal(writer),
        }
    }
}
