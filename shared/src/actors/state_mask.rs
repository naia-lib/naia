use byteorder::WriteBytesExt;
use std::fmt;

use crate::PacketReader;

/// The State Mask is a variable-length byte array, where each bit represents
/// the current state of a Property owned by an Actor. The Property state
/// tracked is whether it has been updated and needs to be synced with the
/// remote Client
#[derive(Debug, Clone)]
pub struct StateMask {
    mask: Vec<u8>,
    bytes: u8,
}

impl StateMask {
    /// Create a new StateMask with a given number of bytes
    pub fn new(bytes: u8) -> StateMask {
        StateMask {
            bytes,
            mask: vec![0; bytes as usize],
        }
    }

    /// Gets the bit at the specified position within the StateMask
    pub fn get_bit(&self, index: u8) -> Option<bool> {
        if let Some(byte) = self.mask.get((index / 8) as usize) {
            let adjusted_index = index % 8;
            return Some(byte & (1 << adjusted_index) != 0);
        }

        return None;
    }

    /// Sets the bit at the specified position within the StateMask
    pub fn set_bit(&mut self, index: u8, value: bool) {
        if let Some(byte) = self.mask.get_mut((index / 8) as usize) {
            let adjusted_index = index % 8;
            let bit_mask = 1 << adjusted_index;
            if value {
                *byte |= bit_mask;
            } else {
                *byte &= !bit_mask;
            }
        }
    }

    /// Clears the whole StateMask
    pub fn clear(&mut self) {
        self.mask = vec![0; self.bytes as usize];
    }

    /// Returns whether any bit has been set in the StateMask
    pub fn is_clear(&self) -> bool {
        for byte in self.mask.iter() {
            if *byte != 0 {
                return false;
            }
        }
        return true;
    }

    /// Get the number of bytes required to represent the StateMask
    pub fn byte_number(&self) -> u8 {
        return self.bytes;
    }

    /// Gets a byte at the specified index in the StateMask
    pub fn get_byte(&self, index: usize) -> u8 {
        return self.mask[index];
    }

    /// Performs a NAND operation on the StateMask, with another StateMask
    pub fn nand(&mut self, other: &StateMask) {
        //if other state mask has different capacity, do nothing
        if other.byte_number() != self.byte_number() {
            return;
        }

        for n in 0..self.bytes {
            if let Some(my_byte) = self.mask.get_mut(n as usize) {
                let other_byte = !other.get_byte(n as usize);
                *my_byte &= other_byte;
            }
        }
    }

    /// Performs an OR operation on the StateMask, with another StateMask
    pub fn or(&mut self, other: &StateMask) {
        //if other state mask has different capacity, do nothing
        if other.byte_number() != self.byte_number() {
            return;
        }

        for n in 0..self.bytes {
            if let Some(my_byte) = self.mask.get_mut(n as usize) {
                let other_byte = other.get_byte(n as usize);
                *my_byte |= other_byte;
            }
        }
    }

    /// Writes the StateMask into an outgoing byte stream
    pub fn write(&mut self, out_bytes: &mut Vec<u8>) {
        out_bytes.write_u8(self.bytes).unwrap();
        for x in 0..self.bytes {
            out_bytes.write_u8(self.mask[x as usize]).unwrap();
        }
    }

    /// Reads the StateMask from an incoming packet
    pub fn read(reader: &mut PacketReader) -> StateMask {
        let bytes: u8 = reader.read_u8();
        let mut mask: Vec<u8> = Vec::new();
        for _ in 0..bytes {
            mask.push(reader.read_u8());
        }
        StateMask { bytes, mask }
    }

    /// Copies the StateMask into another StateMask
    pub fn copy_contents(&mut self, other: &StateMask) {
        //if other state mask has different capacity, do nothing
        if other.byte_number() != self.byte_number() {
            return;
        }

        for n in 0..self.bytes {
            if let Some(my_byte) = self.mask.get_mut(n as usize) {
                let other_byte = other.get_byte(n as usize);
                *my_byte = other_byte;
            }
        }
    }
}

impl fmt::Display for StateMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out_string: String = String::new();
        for y in 0..8 {
            if let Some(bit) = self.get_bit(y) {
                if bit {
                    out_string.push('1');
                } else {
                    out_string.push('0');
                }
            }
        }
        write!(f, "{}", out_string)
    }
}

#[cfg(test)]
mod single_byte_tests {
    use crate::StateMask;

    #[test]
    fn getset() {
        let mut mask = StateMask::new(1);

        mask.set_bit(0, true);
        mask.set_bit(2, true);
        mask.set_bit(4, true);
        mask.set_bit(6, true);
        mask.set_bit(4, false);

        assert!(mask.get_bit(0).unwrap() == true);
        assert!(mask.get_bit(1).unwrap() == false);
        assert!(mask.get_bit(2).unwrap() == true);
        assert!(mask.get_bit(4).unwrap() == false);
        assert!(mask.get_bit(6).unwrap() == true);
    }

    #[test]
    fn clear() {
        let mut mask = StateMask::new(1);

        mask.set_bit(0, true);
        mask.set_bit(2, true);
        mask.set_bit(4, true);
        mask.set_bit(6, true);

        mask.clear();

        assert!(mask.get_bit(0).unwrap() == false);
        assert!(mask.get_bit(2).unwrap() == false);
        assert!(mask.get_bit(4).unwrap() == false);
        assert!(mask.get_bit(6).unwrap() == false);
    }

    #[test]
    fn is_clear_true() {
        let mut mask = StateMask::new(1);

        mask.set_bit(2, true);

        assert!(mask.is_clear() == false);

        mask.set_bit(2, false);

        assert!(mask.is_clear() == true);
    }

    #[test]
    fn bytes() {
        let mut mask = StateMask::new(1);
        assert!(mask.byte_number() == 1);
    }

    #[test]
    fn get_byte() {
        let mut mask = StateMask::new(1);
        mask.set_bit(2, true);
        let byte = mask.get_byte(0);
        assert!(byte == 4);
    }

    #[test]
    fn nand() {
        let mut mask_a = StateMask::new(1);
        mask_a.set_bit(1, true);
        mask_a.set_bit(2, true);

        let mut mask_b = StateMask::new(1);
        mask_b.set_bit(1, true);

        mask_a.nand(&mask_b);

        assert!(mask_a.get_bit(0).unwrap() == false);
        assert!(mask_a.get_bit(1).unwrap() == false);
        assert!(mask_a.get_bit(2).unwrap() == true);
        assert!(mask_a.get_bit(3).unwrap() == false);
    }

    #[test]
    fn or() {
        let mut mask_a = StateMask::new(1);
        mask_a.set_bit(1, true);
        mask_a.set_bit(2, true);

        let mut mask_b = StateMask::new(1);
        mask_b.set_bit(2, true);
        mask_b.set_bit(3, true);

        mask_a.or(&mask_b);

        assert!(mask_a.get_bit(0).unwrap() == false);
        assert!(mask_a.get_bit(1).unwrap() == true);
        assert!(mask_a.get_bit(2).unwrap() == true);
        assert!(mask_a.get_bit(3).unwrap() == true);
        assert!(mask_a.get_bit(4).unwrap() == false);
    }

    #[test]
    fn clone() {
        let mut mask_a = StateMask::new(1);
        mask_a.set_bit(1, true);
        mask_a.set_bit(4, true);

        let mut mask_b = mask_a.clone();

        assert!(mask_b.get_bit(1).unwrap() == true);
        assert!(mask_b.get_bit(3).unwrap() == false);
        assert!(mask_b.get_bit(4).unwrap() == true);
    }
}

#[cfg(test)]
mod double_byte_tests {
    use crate::StateMask;

    #[test]
    fn getset() {
        let mut mask = StateMask::new(2);

        mask.set_bit(0, true);
        mask.set_bit(4, true);
        mask.set_bit(8, true);
        mask.set_bit(12, true);
        mask.set_bit(8, false);

        assert!(mask.get_bit(0).unwrap() == true);
        assert!(mask.get_bit(4).unwrap() == true);
        assert!(mask.get_bit(8).unwrap() == false);
        assert!(mask.get_bit(12).unwrap() == true);
        assert!(mask.get_bit(13).unwrap() == false);
    }

    #[test]
    fn clear() {
        let mut mask = StateMask::new(2);

        mask.set_bit(0, true);
        mask.set_bit(4, true);
        mask.set_bit(8, true);
        mask.set_bit(12, true);

        mask.clear();

        assert!(mask.get_bit(0).unwrap() == false);
        assert!(mask.get_bit(4).unwrap() == false);
        assert!(mask.get_bit(8).unwrap() == false);
        assert!(mask.get_bit(12).unwrap() == false);
    }

    #[test]
    fn is_clear_true() {
        let mut mask = StateMask::new(2);

        mask.set_bit(9, true);

        assert!(mask.is_clear() == false);

        mask.set_bit(9, false);

        assert!(mask.is_clear() == true);
    }

    #[test]
    fn bytes() {
        let mut mask = StateMask::new(2);
        assert!(mask.byte_number() == 2);
    }

    #[test]
    fn get_byte() {
        let mut mask = StateMask::new(2);
        mask.set_bit(10, true);
        let byte = mask.get_byte(1);
        assert!(byte == 4);
    }

    #[test]
    fn nand() {
        let mut mask_a = StateMask::new(2);
        mask_a.set_bit(1, true);
        mask_a.set_bit(2, true);
        mask_a.set_bit(9, true);
        mask_a.set_bit(10, true);

        let mut mask_b = StateMask::new(2);
        mask_b.set_bit(1, true);
        mask_b.set_bit(9, true);

        mask_a.nand(&mask_b);

        assert!(mask_a.get_bit(0).unwrap() == false);
        assert!(mask_a.get_bit(1).unwrap() == false);
        assert!(mask_a.get_bit(2).unwrap() == true);
        assert!(mask_a.get_bit(3).unwrap() == false);

        assert!(mask_a.get_bit(8).unwrap() == false);
        assert!(mask_a.get_bit(9).unwrap() == false);
        assert!(mask_a.get_bit(10).unwrap() == true);
        assert!(mask_a.get_bit(11).unwrap() == false);
    }

    #[test]
    fn or() {
        let mut mask_a = StateMask::new(2);
        mask_a.set_bit(4, true);
        mask_a.set_bit(8, true);

        let mut mask_b = StateMask::new(2);
        mask_b.set_bit(8, true);
        mask_b.set_bit(12, true);

        mask_a.or(&mask_b);

        assert!(mask_a.get_bit(0).unwrap() == false);
        assert!(mask_a.get_bit(4).unwrap() == true);
        assert!(mask_a.get_bit(8).unwrap() == true);
        assert!(mask_a.get_bit(12).unwrap() == true);
        assert!(mask_a.get_bit(15).unwrap() == false);
    }

    #[test]
    fn clone() {
        let mut mask_a = StateMask::new(2);
        mask_a.set_bit(2, true);
        mask_a.set_bit(10, true);

        let mut mask_b = mask_a.clone();

        assert!(mask_b.get_bit(2).unwrap() == true);
        assert!(mask_b.get_bit(4).unwrap() == false);
        assert!(mask_b.get_bit(9).unwrap() == false);
        assert!(mask_b.get_bit(10).unwrap() == true);
    }
}
