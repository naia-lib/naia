pub use naia_serde_derive::{
    Serde, SerdeBevyClient, SerdeBevyServer, SerdeBevyShared, SerdeHecs, SerdeInternal,
};

mod bit_counter;
mod bit_reader;
mod bit_writer;
mod constants;
mod error;
mod impls;
mod integer;
mod outgoing_packet;
mod serde;

pub use bit_counter::BitCounter;
pub use bit_reader::{BitReader, OwnedBitReader};
pub use bit_writer::{BitWrite, BitWriter};
pub use constants::{MTU_SIZE_BITS, MTU_SIZE_BYTES};
pub use error::SerdeErr;
pub use integer::{
    SerdeIntegerConversion, SignedInteger, SignedVariableInteger, UnsignedInteger,
    UnsignedVariableInteger,
};
pub use outgoing_packet::OutgoingPacket;
pub use serde::{
    ConstBitLength, Serde, Serde as SerdeInternal, Serde as SerdeBevyShared,
    Serde as SerdeBevyClient, Serde as SerdeBevyServer, Serde as SerdeHecs,
};
