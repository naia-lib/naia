pub(crate) mod extension_server_name;
pub(crate) mod extension_supported_elliptic_curves;
pub(crate) mod extension_supported_point_formats;
pub(crate) mod extension_supported_signature_algorithms;
pub(crate) mod extension_use_extended_master_secret;
pub(crate) mod renegotiation_info;

use extension_server_name::*;
use extension_supported_elliptic_curves::*;
use extension_supported_point_formats::*;
use extension_supported_signature_algorithms::*;
use extension_use_extended_master_secret::*;

use crate::webrtc::dtls::error::*;

use crate::webrtc::dtls::extension::renegotiation_info::ExtensionRenegotiationInfo;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

// https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xhtml
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExtensionValue {
    ServerName = 0,
    SupportedEllipticCurves = 10,
    SupportedPointFormats = 11,
    SupportedSignatureAlgorithms = 13,
    UseExtendedMasterSecret = 23,
    RenegotiationInfo = 65281,
    Unsupported,
}

impl From<u16> for ExtensionValue {
    fn from(val: u16) -> Self {
        match val {
            0 => ExtensionValue::ServerName,
            10 => ExtensionValue::SupportedEllipticCurves,
            11 => ExtensionValue::SupportedPointFormats,
            13 => ExtensionValue::SupportedSignatureAlgorithms,
            23 => ExtensionValue::UseExtendedMasterSecret,
            65281 => ExtensionValue::RenegotiationInfo,
            _ => ExtensionValue::Unsupported,
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub(crate) enum Extension {
    ServerName(ExtensionServerName),
    SupportedEllipticCurves(ExtensionSupportedEllipticCurves),
    SupportedPointFormats(ExtensionSupportedPointFormats),
    SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms),
    UseExtendedMasterSecret(ExtensionUseExtendedMasterSecret),
    RenegotiationInfo(ExtensionRenegotiationInfo),
}

impl Extension {
    pub(crate) fn extension_value(&self) -> ExtensionValue {
        match self {
            Extension::ServerName(ext) => ext.extension_value(),
            Extension::SupportedEllipticCurves(ext) => ext.extension_value(),
            Extension::SupportedPointFormats(ext) => ext.extension_value(),
            Extension::SupportedSignatureAlgorithms(ext) => ext.extension_value(),
            Extension::UseExtendedMasterSecret(ext) => ext.extension_value(),
            Extension::RenegotiationInfo(ext) => ext.extension_value(),
        }
    }

    pub(crate) fn size(&self) -> usize {
        let mut len = 2;

        len += match self {
            Extension::ServerName(ext) => ext.size(),
            Extension::SupportedEllipticCurves(ext) => ext.size(),
            Extension::SupportedPointFormats(ext) => ext.size(),
            Extension::SupportedSignatureAlgorithms(ext) => ext.size(),
            Extension::UseExtendedMasterSecret(ext) => ext.size(),
            Extension::RenegotiationInfo(ext) => ext.size(),
        };

        len
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(self.extension_value() as u16)?;
        match self {
            Extension::ServerName(ext) => ext.marshal(writer),
            Extension::SupportedEllipticCurves(ext) => ext.marshal(writer),
            Extension::SupportedPointFormats(ext) => ext.marshal(writer),
            Extension::SupportedSignatureAlgorithms(ext) => ext.marshal(writer),
            Extension::UseExtendedMasterSecret(ext) => ext.marshal(writer),
            Extension::RenegotiationInfo(ext) => ext.marshal(writer),
        }
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let extension_value: ExtensionValue = reader.read_u16::<BigEndian>()?.into();
        match extension_value {
            ExtensionValue::ServerName => Ok(Extension::ServerName(
                ExtensionServerName::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedEllipticCurves => Ok(Extension::SupportedEllipticCurves(
                ExtensionSupportedEllipticCurves::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedPointFormats => Ok(Extension::SupportedPointFormats(
                ExtensionSupportedPointFormats::unmarshal(reader)?,
            )),
            ExtensionValue::SupportedSignatureAlgorithms => {
                Ok(Extension::SupportedSignatureAlgorithms(
                    ExtensionSupportedSignatureAlgorithms::unmarshal(reader)?,
                ))
            }
            ExtensionValue::UseExtendedMasterSecret => Ok(Extension::UseExtendedMasterSecret(
                ExtensionUseExtendedMasterSecret::unmarshal(reader)?,
            )),
            ExtensionValue::RenegotiationInfo => Ok(Extension::RenegotiationInfo(
                ExtensionRenegotiationInfo::unmarshal(reader)?,
            )),
            _ => Err(Error::ErrInvalidExtensionType),
        }
    }
}
