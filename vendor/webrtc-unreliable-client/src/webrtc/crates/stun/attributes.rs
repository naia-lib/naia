use crate::webrtc::stun::error::*;
use crate::webrtc::stun::message::*;

use std::fmt;

/// Attributes is list of message attributes.
#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub(crate) struct Attributes(pub(crate) Vec<RawAttribute>);

impl Attributes {
    /// get returns first attribute from list by the type.
    /// If attribute is present the RawAttribute is returned and the
    /// boolean is true. Otherwise the returned RawAttribute will be
    /// empty and boolean will be false.
    pub(crate) fn get(&self, t: AttrType) -> (RawAttribute, bool) {
        for candidate in &self.0 {
            if candidate.typ == t {
                return (candidate.clone(), true);
            }
        }

        (RawAttribute::default(), false)
    }
}

/// AttrType is attribute type.
#[derive(PartialEq, Debug, Eq, Default, Copy, Clone)]
pub(crate) struct AttrType(pub(crate) u16);

impl fmt::Display for AttrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let other = format!("0x{:x}", self.0);

        let s = match *self {
            ATTR_MAPPED_ADDRESS => "MAPPED-ADDRESS",
            ATTR_USERNAME => "USERNAME",
            ATTR_ERROR_CODE => "ERROR-CODE",
            ATTR_MESSAGE_INTEGRITY => "MESSAGE-INTEGRITY",
            ATTR_UNKNOWN_ATTRIBUTES => "UNKNOWN-ATTRIBUTES",
            ATTR_REALM => "REALM",
            ATTR_NONCE => "NONCE",
            ATTR_XORMAPPED_ADDRESS => "XOR-MAPPED-ADDRESS",
            ATTR_SOFTWARE => "SOFTWARE",
            ATTR_ALTERNATE_SERVER => "ALTERNATE-SERVER",
            ATTR_FINGERPRINT => "FINGERPRINT",
            ATTR_PRIORITY => "PRIORITY",
            ATTR_USE_CANDIDATE => "USE-CANDIDATE",
            ATTR_ICE_CONTROLLED => "ICE-CONTROLLED",
            ATTR_ICE_CONTROLLING => "ICE-CONTROLLING",
            ATTR_CHANNEL_NUMBER => "CHANNEL-NUMBER",
            ATTR_LIFETIME => "LIFETIME",
            ATTR_XOR_PEER_ADDRESS => "XOR-PEER-ADDRESS",
            ATTR_DATA => "DATA",
            ATTR_XOR_RELAYED_ADDRESS => "XOR-RELAYED-ADDRESS",
            ATTR_EVEN_PORT => "EVEN-PORT",
            ATTR_REQUESTED_TRANSPORT => "REQUESTED-TRANSPORT",
            ATTR_DONT_FRAGMENT => "DONT-FRAGMENT",
            ATTR_RESERVATION_TOKEN => "RESERVATION-TOKEN",
            ATTR_CONNECTION_ID => "CONNECTION-ID",
            ATTR_REQUESTED_ADDRESS_FAMILY => "REQUESTED-ADDRESS-FAMILY",
            ATTR_MESSAGE_INTEGRITY_SHA256 => "MESSAGE-INTEGRITY-SHA256",
            ATTR_PASSWORD_ALGORITHM => "PASSWORD-ALGORITHM",
            ATTR_USER_HASH => "USERHASH",
            ATTR_PASSWORD_ALGORITHMS => "PASSWORD-ALGORITHMS",
            ATTR_ALTERNATE_DOMAIN => "ALTERNATE-DOMAIN",
            _ => other.as_str(),
        };

        write!(f, "{}", s)
    }
}

impl AttrType {
    /// value returns uint16 representation of attribute type.
    pub(crate) fn value(&self) -> u16 {
        self.0
    }
}

/// Attributes from comprehension-required range (0x0000-0x7FFF).
pub(crate) const ATTR_MAPPED_ADDRESS: AttrType = AttrType(0x0001); // MAPPED-ADDRESS
pub(crate) const ATTR_USERNAME: AttrType = AttrType(0x0006); // USERNAME
pub(crate) const ATTR_MESSAGE_INTEGRITY: AttrType = AttrType(0x0008); // MESSAGE-INTEGRITY
pub(crate) const ATTR_ERROR_CODE: AttrType = AttrType(0x0009); // ERROR-CODE
pub(crate) const ATTR_UNKNOWN_ATTRIBUTES: AttrType = AttrType(0x000A); // UNKNOWN-ATTRIBUTES
pub(crate) const ATTR_REALM: AttrType = AttrType(0x0014); // REALM
pub(crate) const ATTR_NONCE: AttrType = AttrType(0x0015); // NONCE
pub(crate) const ATTR_XORMAPPED_ADDRESS: AttrType = AttrType(0x0020); // XOR-MAPPED-ADDRESS

/// Attributes from comprehension-optional range (0x8000-0xFFFF).
pub(crate) const ATTR_SOFTWARE: AttrType = AttrType(0x8022); // SOFTWARE
pub(crate) const ATTR_ALTERNATE_SERVER: AttrType = AttrType(0x8023); // ALTERNATE-SERVER
pub(crate) const ATTR_FINGERPRINT: AttrType = AttrType(0x8028); // FINGERPRINT

/// Attributes from RFC 5245 ICE.
pub(crate) const ATTR_PRIORITY: AttrType = AttrType(0x0024); // PRIORITY
pub(crate) const ATTR_USE_CANDIDATE: AttrType = AttrType(0x0025); // USE-CANDIDATE
pub(crate) const ATTR_ICE_CONTROLLED: AttrType = AttrType(0x8029); // ICE-CONTROLLED
pub(crate) const ATTR_ICE_CONTROLLING: AttrType = AttrType(0x802A); // ICE-CONTROLLING

/// Attributes from RFC 5766 TURN.
pub(crate) const ATTR_CHANNEL_NUMBER: AttrType = AttrType(0x000C); // CHANNEL-NUMBER
pub(crate) const ATTR_LIFETIME: AttrType = AttrType(0x000D); // LIFETIME
pub(crate) const ATTR_XOR_PEER_ADDRESS: AttrType = AttrType(0x0012); // XOR-PEER-ADDRESS
pub(crate) const ATTR_DATA: AttrType = AttrType(0x0013); // DATA
pub(crate) const ATTR_XOR_RELAYED_ADDRESS: AttrType = AttrType(0x0016); // XOR-RELAYED-ADDRESS
pub(crate) const ATTR_EVEN_PORT: AttrType = AttrType(0x0018); // EVEN-PORT
pub(crate) const ATTR_REQUESTED_TRANSPORT: AttrType = AttrType(0x0019); // REQUESTED-TRANSPORT
pub(crate) const ATTR_DONT_FRAGMENT: AttrType = AttrType(0x001A); // DONT-FRAGMENT
pub(crate) const ATTR_RESERVATION_TOKEN: AttrType = AttrType(0x0022); // RESERVATION-TOKEN

/// Attributes from RFC 6062 TURN Extensions for TCP Allocations.
pub(crate) const ATTR_CONNECTION_ID: AttrType = AttrType(0x002a); // CONNECTION-ID

/// Attributes from RFC 6156 TURN IPv6.
pub(crate) const ATTR_REQUESTED_ADDRESS_FAMILY: AttrType = AttrType(0x0017); // REQUESTED-ADDRESS-FAMILY

/// Attributes from RFC 8489 STUN.
pub(crate) const ATTR_MESSAGE_INTEGRITY_SHA256: AttrType = AttrType(0x001C); // MESSAGE-INTEGRITY-SHA256
pub(crate) const ATTR_PASSWORD_ALGORITHM: AttrType = AttrType(0x001D); // PASSWORD-ALGORITHM
pub(crate) const ATTR_USER_HASH: AttrType = AttrType(0x001E); // USER-HASH
pub(crate) const ATTR_PASSWORD_ALGORITHMS: AttrType = AttrType(0x8002); // PASSWORD-ALGORITHMS
pub(crate) const ATTR_ALTERNATE_DOMAIN: AttrType = AttrType(0x8003); // ALTERNATE-DOMAIN

/// RawAttribute is a Type-Length-Value (TLV) object that
/// can be added to a STUN message. Attributes are divided into two
/// types: comprehension-required and comprehension-optional.  STUN
/// agents can safely ignore comprehension-optional attributes they
/// don't understand, but cannot successfully process a message if it
/// contains comprehension-required attributes that are not
/// understood.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawAttribute {
    pub(crate) typ: AttrType,
    pub(crate) length: u16, // ignored while encoding
    pub(crate) value: Vec<u8>,
}

impl fmt::Display for RawAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.typ, self.value)
    }
}

impl Setter for RawAttribute {
    /// add_to implements Setter, adding attribute as a.Type with a.Value and ignoring
    /// the Length field.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        m.add(self.typ, &self.value);
        Ok(())
    }
}

pub(crate) const PADDING: usize = 4;

/// STUN aligns attributes on 32-bit boundaries, attributes whose content
/// is not a multiple of 4 bytes are padded with 1, 2, or 3 bytes of
/// padding so that its value contains a multiple of 4 bytes.  The
/// padding bits are ignored, and may be any value.
///
/// https://tools.ietf.org/html/rfc5389#section-15
pub(crate) fn nearest_padded_value_length(l: usize) -> usize {
    let mut n = PADDING * (l / PADDING);
    if n < l {
        n += PADDING
    }
    n
}

/// This method converts uint16 vlue to AttrType. If it finds an old attribute
/// type value, it also translates it to the new value to enable backward
/// compatibility. (See: https://github.com/pion/stun/issues/21)
pub(crate) fn compat_attr_type(val: u16) -> AttrType {
    if val == 0x8020 {
        // draft-ietf-behave-rfc3489bis-02, MS-TURN
        ATTR_XORMAPPED_ADDRESS // new: 0x0020 (from draft-ietf-behave-rfc3489bis-03 on)
    } else {
        AttrType(val)
    }
}
