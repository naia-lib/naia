use super::{reader_writer::{BitReader, BitWriter}, error::DeErr};

/// A trait for objects that can be serialized to a bitstream.
pub trait Ser {
    /// Serialize Self to a BitWriter
    fn ser(&self, bit_writer: &mut BitWriter);
}

/// A trait for objects that can be deserialized from a bitstream.
pub trait De: Sized {
    /// Parse Self from a BitReader
    fn de(bit_reader: &mut BitReader) -> Result<Self, DeErr>;
}
