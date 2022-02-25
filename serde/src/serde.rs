use super::{reader_writer::{BitReader, BitWriter}, error::SerdeErr};

/// A trait for objects that can be serialized to a bitstream.
pub trait Serde: Sized {
    /// Serialize Self to a BitWriter
    fn ser(&self, bit_writer: &mut BitWriter);

    /// Parse Self from a BitReader
    fn de(bit_reader: &mut BitReader) -> Result<Self, SerdeErr>;
}