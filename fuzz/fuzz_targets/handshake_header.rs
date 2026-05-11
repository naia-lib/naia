#![no_main]
use libfuzzer_sys::fuzz_target;

use naia_serde::{BitReader, Serde};
use naia_shared::handshake::HandshakeHeader;

fuzz_target!(|data: &[u8]| {
    let mut reader = BitReader::new(data);
    let _ = HandshakeHeader::de(&mut reader);
});
