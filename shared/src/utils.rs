use naia_serde::{BitReader, Serde};

use crate::standard_header::StandardHeader;

/// Strip the standard header off of a packet's payload
pub fn read_header(reader: &mut BitReader) {
    StandardHeader::de(reader).unwrap();
}
