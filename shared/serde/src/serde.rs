
use super::{
    error::{SerdeErr, WriteOverflowError},
    reader_writer::{BitReader, BitWrite},
};

/// A trait for objects that can be serialized to a bitstream.
pub trait Serde: Sized + Clone + PartialEq {
    /// Serialize Self to a BitWriter
    fn ser(&self, writer: &mut dyn BitWrite) -> Result<(), WriteOverflowError>;

    /// Parse Self from a BitReader
    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr>;
}
