pub use naia_serde_derive::{Serde, SerdeInternal};

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
#[cfg(feature = "bench_instrumentation")]
pub use bit_writer::bench_serde_counters;
pub use bit_writer::{
    BitWrite, BitWriter, CachedComponentUpdate, VecBitWriter, CACHED_UPDATE_BITS,
    CACHED_UPDATE_BYTES,
};
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
    wire_schema_count, wire_schema_custom_leaf, wire_schema_field, wire_schema_label, WireSchema,
    WireSchemaContext, SCHEMA_NATIVE_ENDIAN, SCHEMA_ORDERED, SCHEMA_TAG_ARRAY, SCHEMA_TAG_BACKREF,
    SCHEMA_TAG_BOOL, SCHEMA_TAG_BYTES, SCHEMA_TAG_CHAR, SCHEMA_TAG_CUSTOM_LEAF,
    SCHEMA_TAG_ENTITY_PROPERTY, SCHEMA_TAG_ENUM, SCHEMA_TAG_FLOAT, SCHEMA_TAG_HASH_MAP,
    SCHEMA_TAG_HASH_SET, SCHEMA_TAG_INTEGER, SCHEMA_TAG_NATIVE, SCHEMA_TAG_OPTION,
    SCHEMA_TAG_PHANTOM, SCHEMA_TAG_STRING, SCHEMA_TAG_STRUCT, SCHEMA_TAG_TUPLE, SCHEMA_TAG_UNIT,
    SCHEMA_TAG_VECTOR, SCHEMA_UNORDERED, WIRE_SCHEMA_DOMAIN,
};
pub use serde::{
    ConstBitLength, MaxBits, MaxBitsFallback, Serde, Serde as SerdeBevyClient,
    Serde as SerdeBevyServer, Serde as SerdeBevyShared, Serde as SerdeInternal,
    UNBOUNDED_BIT_LENGTH,
};
