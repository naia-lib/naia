use bitvec::{array::BitArray, boxed::BitBox, order::Lsb0, slice::BitSlice, vec::BitVec};

pub type BVec = BitVec<u8, Lsb0>;
pub type BArray = BitArray<u8, Lsb0>;
pub type BSlice = BitSlice<u8, Lsb0>;
pub type BBox = BitBox<u8, Lsb0>;
