use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use crate::{reader_writer::{BitReader, BitWriter}, error::DeErr, traits::{De, Ser}, UnsignedVariableInteger};

impl<K: Ser> Ser for HashSet<K>
{
    fn ser(&self, writer: &mut BitWriter) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        writer.write(&length);
        for value in self {
            writer.write(value);
        }
    }
}

impl<K: De + Eq + Hash> De for HashSet<K>
{
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
        let length_int: UnsignedVariableInteger<5> = reader.read().unwrap();
        let length_usize = length_int.get() as usize;
        let mut output: HashSet<K> = HashSet::new();
        for _ in 0..length_usize {
            let value = reader.read().unwrap();
            output.insert(value);
        }
        Ok(output)
    }
}

impl<K: Ser, V: Ser> Ser for HashMap<K, V>
{
    fn ser(&self, writer: &mut BitWriter) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        writer.write(&length);
        for (key, value) in self {
            writer.write(key);
            writer.write(value);
        }
    }
}

impl<K: De + Eq + Hash, V: De> De for HashMap<K, V>
{
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
        let length_int: UnsignedVariableInteger<5> = reader.read().unwrap();
        let length_usize = length_int.get() as usize;
        let mut output: HashMap<K, V> = HashMap::new();
        for _ in 0..length_usize {
            let key = reader.read().unwrap();
            let value = reader.read().unwrap();
            output.insert(key, value);
        }
        Ok(output)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use crate::{BitReader, BitWriter};

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

        writer.write(&in_1);
        writer.write(&in_2);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1: HashMap<i32, String> = reader.read().unwrap();
        let out_2: HashMap<u16, bool> = reader.read().unwrap();

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

        writer.write(&in_1);
        writer.write(&in_2);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1: HashSet<i32> = reader.read().unwrap();
        let out_2: HashSet<u16> = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}
