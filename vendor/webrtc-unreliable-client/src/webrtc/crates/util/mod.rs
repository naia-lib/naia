use std::io;
use thiserror::Error;

pub(crate) mod fixed_big_int;
pub(crate) mod replay_detector;

/// Possible errors while exporting keying material.
///
/// These errors might have been more logically kept in the dtls
/// crate, but that would have required a direct depdency between
/// srtp and dtls.
#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub(crate) enum KeyingMaterialExporterError {
    #[error("export_keying_material io: {0}")]
    Io(#[source] error::IoError),
}

impl From<io::Error> for KeyingMaterialExporterError {
    fn from(e: io::Error) -> Self {
        KeyingMaterialExporterError::Io(error::IoError(e))
    }
}

pub(crate) mod buffer;
pub(crate) mod conn;
pub(crate) mod ifaces;
pub(crate) mod marshal;
pub(crate) mod notify;
pub(crate) mod vnet;
pub(crate) use crate::webrtc::util::buffer::Buffer;
pub(crate) use crate::webrtc::util::conn::Conn;

mod error;
pub(crate) use error::Error;
