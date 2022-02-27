use super::{
    error::SerdeErr,
    reader_writer::{BitReader, BitWrite},
};

/// A trait for objects that can be serialized to a bitstream.
pub trait Serde: Sized + Clone + PartialEq {
    /// Serialize Self to a BitWriter
    fn ser<S: BitWrite>(&self, writer: &mut S);

    /// Parse Self from a BitReader
    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr>;
}
