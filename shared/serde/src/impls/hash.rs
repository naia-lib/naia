use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        wire_schema_field, Serde, WireSchema, WireSchemaContext, SCHEMA_TAG_HASH_MAP,
        SCHEMA_TAG_HASH_SET, SCHEMA_UNORDERED,
    },
    UnsignedVariableInteger,
};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

impl<K: Serde + Eq + Hash> Serde for HashSet<K> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        length.ser(writer);
        for value in self {
            value.ser(writer);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        let mut output: HashSet<K> = HashSet::new();
        for _ in 0..length_usize {
            let value = K::de(reader)?;
            output.insert(value);
        }
        Ok(output)
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        output += length.bit_length();
        for value in self {
            output += value.bit_length();
        }
        output
    }
}

impl<K: Serde + Eq + Hash, V: Serde> Serde for HashMap<K, V> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        length.ser(writer);
        for (key, value) in self {
            key.ser(writer);
            value.ser(writer);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        let mut output: HashMap<K, V> = HashMap::new();
        for _ in 0..length_usize {
            let key = K::de(reader)?;
            let value = V::de(reader)?;
            output.insert(key, value);
        }
        Ok(output)
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        output += length.bit_length();
        for (key, value) in self {
            output += key.bit_length();
            output += value.bit_length();
        }
        output
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn read_write_hash_map() {
        // Write
        let mut writer = BitWriter::new();

        let mut in_1 = HashMap::<i32, String>::new();
        in_1.insert(-7, "negative seven".to_string());
        in_1.insert(331, "three hundred and thiry-one".to_string());
        in_1.insert(-65, "negative sixty-five".to_string());
        let mut in_2 = HashMap::<u16, bool>::new();
        in_2.insert(5, true);
        in_2.insert(73, false);
        in_2.insert(44, false);
        in_2.insert(21, true);
        in_2.insert(67, false);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = HashMap::<i32, String>::de(&mut reader).unwrap();
        let out_2 = HashMap::<u16, bool>::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }

    #[test]
    fn read_write_hash_set() {
        // Write
        let mut writer = BitWriter::new();

        let mut in_1 = HashSet::<i32>::new();
        in_1.insert(-7);
        in_1.insert(331);
        in_1.insert(-65);
        let mut in_2 = HashSet::<u16>::new();
        in_2.insert(5);
        in_2.insert(73);
        in_2.insert(44);
        in_2.insert(21);
        in_2.insert(67);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = HashSet::<i32>::de(&mut reader).unwrap();
        let out_2 = HashSet::<u16>::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// Unordered collections: the unordered class plus key/value (or element)
// descriptors plus the exact `UnsignedVariableInteger<5>` length codec. The
// class byte is what keeps an ordered `Vec<u8>` and an unordered
// `HashSet<u8>` from ever describing alike.
impl<K: WireSchema + Eq + std::hash::Hash> WireSchema for HashSet<K> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_HASH_SET);
        out.push(SCHEMA_UNORDERED);
        wire_schema_field::<K>(ctx, out);
        wire_schema_field::<UnsignedVariableInteger<5>>(ctx, out);
    }
}

impl<K: WireSchema + Eq + std::hash::Hash, V: WireSchema> WireSchema for HashMap<K, V> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_HASH_MAP);
        out.push(SCHEMA_UNORDERED);
        wire_schema_field::<K>(ctx, out);
        wire_schema_field::<V>(ctx, out);
        wire_schema_field::<UnsignedVariableInteger<5>>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{WireSchema, SCHEMA_TAG_HASH_SET, SCHEMA_TAG_INTEGER, SCHEMA_UNORDERED};
    use std::collections::HashSet;

    /// The unordered class separates hash collections from ordered vectors
    /// with identical elements and codecs.
    #[test]
    fn unordered_class_separates_hash_collections() {
        let bytes = HashSet::<u8>::wire_schema_bytes();
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len()], SCHEMA_TAG_HASH_SET);
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len() + 1], SCHEMA_UNORDERED);
        assert_eq!(
            &bytes[WIRE_SCHEMA_DOMAIN.len() + 2..],
            &[SCHEMA_TAG_INTEGER, 0, 0, 8, SCHEMA_TAG_INTEGER, 0, 1, 5][..],
        );
        assert_ne!(
            bytes,
            Vec::<u8>::wire_schema_bytes(),
            "unordered set must differ from ordered vector",
        );
    }
}
