use std::fmt;

use crate::webrtc::sdp::description::common::*;

/// MediaDescription represents a media type.
/// <https://tools.ietf.org/html/rfc4566#section-5.14>
#[derive(Debug, Default, Clone)]
pub(crate) struct MediaDescription {
    /// `m=<media> <port>/<number of ports> <proto> <fmt> ...`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.14>
    pub(crate) media_name: MediaName,

    /// `i=<session description>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.4>
    pub(crate) media_title: Option<Information>,

    /// `c=<nettype> <addrtype> <connection-address>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.7>
    pub(crate) connection_information: Option<ConnectionInformation>,

    /// `b=<bwtype>:<bandwidth>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.8>
    pub(crate) bandwidth: Vec<Bandwidth>,

    /// `k=<method>`
    ///
    /// `k=<method>:<encryption key>`
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.12>
    pub(crate) encryption_key: Option<EncryptionKey>,

    /// Attributes are the primary means for extending SDP.  Attributes may
    /// be defined to be used as "session-level" attributes, "media-level"
    /// attributes, or both.
    ///
    /// <https://tools.ietf.org/html/rfc4566#section-5.12>
    pub(crate) attributes: Vec<Attribute>,
}

impl MediaDescription {
    /// attribute returns the value of an attribute and if it exists
    pub(crate) fn attribute(&self, key: &str) -> Option<Option<&str>> {
        for a in &self.attributes {
            if a.key == key {
                return Some(a.value.as_ref().map(|s| s.as_ref()));
            }
        }
        None
    }

    /// with_property_attribute adds a property attribute 'a=key' to the media description
    pub(crate) fn with_property_attribute(mut self, key: String) -> Self {
        self.attributes.push(Attribute::new(key, None));
        self
    }

    /// with_value_attribute adds a value attribute 'a=key:value' to the media description
    pub(crate) fn with_value_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.push(Attribute::new(key, Some(value)));
        self
    }

    /// with_fingerprint adds a fingerprint to the media description
    pub(crate) fn with_fingerprint(self, algorithm: String, value: String) -> Self {
        self.with_value_attribute("fingerprint".to_owned(), algorithm + " " + &value)
    }

    /// with_ice_credentials adds ICE credentials to the media description
    pub(crate) fn with_ice_credentials(self, username: String, password: String) -> Self {
        self.with_value_attribute("ice-ufrag".to_string(), username)
            .with_value_attribute("ice-pwd".to_string(), password)
    }
}

/// RangedPort supports special format for the media field "m=" port value. If
/// it may be necessary to specify multiple transport ports, the protocol allows
/// to write it as: <port>/<number of ports> where number of ports is a an
/// offsetting range.
#[derive(Debug, Default, Clone)]
pub(crate) struct RangedPort {
    pub(crate) value: isize,
    pub(crate) range: Option<isize>,
}

impl fmt::Display for RangedPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(range) = self.range {
            write!(f, "{}/{}", self.value, range)
        } else {
            write!(f, "{}", self.value)
        }
    }
}

/// MediaName describes the "m=" field storage structure.
#[derive(Debug, Default, Clone)]
pub(crate) struct MediaName {
    pub(crate) media: String,
    pub(crate) port: RangedPort,
    pub(crate) protos: Vec<String>,
    pub(crate) formats: Vec<String>,
}

impl fmt::Display for MediaName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = [
            self.media.clone(),
            self.port.to_string(),
            self.protos.join("/"),
            self.formats.join(" "),
        ];
        write!(f, "{}", s.join(" "))
    }
}
