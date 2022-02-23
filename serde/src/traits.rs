use super::{
    error::DeErr,
    types::{BSlice, BVec},
};

/// A trait for objects that can be serialized to a bitstream.
pub trait Ser {
    /// Serialize Self to bits.
    ///
    /// This is a convenient wrapper around `ser`.
    fn serialize(&self) -> BVec {
        let mut s = BVec::new();
        self.ser(&mut s);
        s
    }

    /// Serialize Self to bits.
    fn ser(&self, output: &mut BVec);
}

/// A trait for objects that can be deserialized from a bitstream.
pub trait De: Sized {
    /// Parse Self from the input bits.
    ///
    /// This is a convenient wrapper around `de`.
    fn deserialize(d: &BSlice) -> Result<Self, DeErr> {
        De::de(&mut 0, d)
    }

    /// Parse Self from the input bits starting at index `offset`.
    ///
    /// After deserialization, `offset` is updated to point at the bit after
    /// the last one used.
    fn de(offset: &mut usize, bytes: &BSlice) -> Result<Self, DeErr>;
}
