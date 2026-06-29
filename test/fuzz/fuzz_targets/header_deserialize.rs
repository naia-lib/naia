#![no_main]
use libfuzzer_sys::fuzz_target;

use naia_serde::{BitReader, Serde};
use naia_shared::StandardHeader;

fuzz_target!(|data: &[u8]| {
    let mut reader = BitReader::new(data);
    let _ = StandardHeader::de(&mut reader);
});
