#![no_main]
use libfuzzer_sys::fuzz_target;

use naia_serde::{
    BitReader, Serde, SignedInteger, SignedVariableInteger, UnsignedInteger, UnsignedVariableInteger,
};

fuzz_target!(|data: &[u8]| {
    macro_rules! try_de {
        ($t:ty) => {{
            let mut reader = BitReader::new(data);
            let _ = <$t>::de(&mut reader);
        }};
    }

    // Scalar types
    try_de!(bool);
    try_de!(u8);
    try_de!(u16);
    try_de!(u32);
    try_de!(u64);
    try_de!(i8);
    try_de!(i16);
    try_de!(i32);
    try_de!(i64);
    try_de!(f32);
    try_de!(f64);

    // Collections
    try_de!(Option<u32>);
    try_de!(Vec<u8>);
    try_de!(String);

    // SerdeInteger family
    try_de!(UnsignedInteger<8>);
    try_de!(UnsignedInteger<16>);
    try_de!(SignedInteger<12>);
    try_de!(UnsignedVariableInteger<3>);
    try_de!(UnsignedVariableInteger<5>);
    try_de!(SignedVariableInteger<4>);
});
