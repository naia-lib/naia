use crate::webrtc::stun::attributes::ATTR_FINGERPRINT;
use crate::webrtc::stun::error::*;
use crate::webrtc::stun::message::*;

use crc::{Crc, CRC_32_ISO_HDLC};

// FingerprintAttr represents FINGERPRINT attribute.
//
// RFC 5389 Section 15.5
pub(crate) struct FingerprintAttr;

// FINGERPRINT is shorthand for FingerprintAttr.
//
// Example:
//
//  m := New()
//  FINGERPRINT.add_to(m)
pub(crate) const FINGERPRINT: FingerprintAttr = FingerprintAttr {};

pub(crate) const FINGERPRINT_XOR_VALUE: u32 = 0x5354554e;
pub(crate) const FINGERPRINT_SIZE: usize = 4; // 32 bit

// FingerprintValue returns CRC-32 of b XOR-ed by 0x5354554e.
//
// The value of the attribute is computed as the CRC-32 of the STUN message
// up to (but excluding) the FINGERPRINT attribute itself, XOR'ed with
// the 32-bit value 0x5354554e (the XOR helps in cases where an
// application packet is also using CRC-32 in it).
pub(crate) fn fingerprint_value(b: &[u8]) -> u32 {
    let checksum = Crc::<u32>::new(&CRC_32_ISO_HDLC).checksum(b);
    checksum ^ FINGERPRINT_XOR_VALUE // XOR
}

impl Setter for FingerprintAttr {
    // add_to adds fingerprint to message.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        let l = m.length;
        // length in header should include size of fingerprint attribute
        m.length += (FINGERPRINT_SIZE + ATTRIBUTE_HEADER_SIZE) as u32; // increasing length
        m.write_length(); // writing Length to Raw
        let val = fingerprint_value(&m.raw);
        let b = val.to_be_bytes();
        m.length = l;
        m.add(ATTR_FINGERPRINT, &b);
        Ok(())
    }
}
