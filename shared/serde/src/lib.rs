pub use naia_serde_derive::{
    Serde, SerdeBevyClient, SerdeBevyServer, SerdeBevyShared, SerdeInternal,
};

mod bit_counter;
mod bit_reader;
mod bit_writer;
mod constants;
mod error;
mod file_bit_writer;
mod impls;
mod number;
mod outgoing_packet;
mod serde;

pub use bit_counter::BitCounter;
pub use bit_reader::{BitReader, OwnedBitReader};
pub use bit_writer::{BitWrite, BitWriter, CachedComponentUpdate};
#[cfg(feature = "bench_instrumentation")]
pub use bit_writer::bench_serde_counters;
pub use constants::{MTU_SIZE_BITS, MTU_SIZE_BYTES};
pub use error::SerdeErr;
pub use file_bit_writer::FileBitWriter;
pub use number::{
    SerdeFloatConversion, SerdeIntegerConversion, SignedFloat, SignedInteger, SignedVariableFloat,
    SignedVariableInteger, UnsignedFloat, UnsignedInteger, UnsignedVariableFloat,
    UnsignedVariableInteger,
};
pub use outgoing_packet::OutgoingPacket;
pub use serde::{
    ConstBitLength, Serde, Serde as SerdeBevyClient, Serde as SerdeBevyServer,
    Serde as SerdeBevyShared, Serde as SerdeInternal,
};
