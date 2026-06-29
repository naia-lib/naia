#![no_main]
use libfuzzer_sys::fuzz_target;

use naia_serde::{
    BitReader, BitWriter, Serde, SignedFloat, SignedVariableFloat, UnsignedFloat,
    UnsignedVariableFloat,
};

fuzz_target!(|data: &[u8]| {
    macro_rules! try_de {
        ($t:ty) => {{
            let mut reader = BitReader::new(data);
            let _ = <$t>::de(&mut reader);
        }};
    }

    // Roundtrip: deserialize → re-serialize → deserialize again; both de calls must agree.
    macro_rules! try_roundtrip {
        ($t:ty) => {{
            let mut reader = BitReader::new(data);
            if let Ok(first) = <$t>::de(&mut reader) {
                let mut writer = BitWriter::with_max_capacity();
                first.ser(&mut writer);
                let packet = writer.to_packet();
                let mut reader2 = BitReader::new(packet.slice());
                if let Ok(second) = <$t>::de(&mut reader2) {
                    assert_eq!(first, second, "roundtrip mismatch for {}", stringify!($t));
                }
            }
        }};
    }

    // --- UnsignedFloat ---
    try_de!(UnsignedFloat<4, 1>);
    try_de!(UnsignedFloat<7, 1>);
    try_de!(UnsignedFloat<8, 2>);
    try_de!(UnsignedFloat<12, 3>);
    try_de!(UnsignedFloat<16, 4>);
    try_de!(UnsignedFloat<20, 2>);

    try_roundtrip!(UnsignedFloat<8, 2>);
    try_roundtrip!(UnsignedFloat<16, 4>);

    // --- SignedFloat ---
    try_de!(SignedFloat<4, 1>);
    try_de!(SignedFloat<7, 1>);
    try_de!(SignedFloat<8, 2>);
    try_de!(SignedFloat<12, 3>);
    try_de!(SignedFloat<16, 4>);
    try_de!(SignedFloat<20, 2>);

    try_roundtrip!(SignedFloat<8, 2>);
    try_roundtrip!(SignedFloat<16, 4>);

    // --- UnsignedVariableFloat ---
    try_de!(UnsignedVariableFloat<2, 1>);
    try_de!(UnsignedVariableFloat<3, 1>);
    try_de!(UnsignedVariableFloat<5, 2>);
    try_de!(UnsignedVariableFloat<7, 3>);

    try_roundtrip!(UnsignedVariableFloat<5, 2>);

    // --- SignedVariableFloat ---
    try_de!(SignedVariableFloat<2, 1>);
    try_de!(SignedVariableFloat<5, 1>);
    try_de!(SignedVariableFloat<6, 2>);
    try_de!(SignedVariableFloat<8, 3>);

    try_roundtrip!(SignedVariableFloat<5, 1>);
});
