
use naia_serde::{BitReader, BitWriter, SignedInteger, UnsignedInteger, UnsignedVariableInteger};

#[test]
fn read_write_unsigned() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = UnsignedInteger::<7>::new(123);
    let in_2 = UnsignedInteger::<20>::new(535221);
    let in_3 = UnsignedInteger::<2>::new(3);

    writer.write(&in_1);
    writer.write(&in_2);
    writer.write(&in_3);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();
    let out_2 = reader.read().unwrap();
    let out_3 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
}

#[test]
fn read_write_signed() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SignedInteger::<10>::new(-668);
    let in_2 = SignedInteger::<20>::new(53);
    let in_3 = SignedInteger::<2>::new(-3);

    writer.write(&in_1);
    writer.write(&in_2);
    writer.write(&in_3);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();
    let out_2 = reader.read().unwrap();
    let out_3 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
}

#[test]
fn read_write_unsigned_variable() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = UnsignedVariableInteger::<3>::new(23);
    let in_2 = UnsignedVariableInteger::<5>::new(153);
    let in_3 = UnsignedVariableInteger::<2>::new(3);

    writer.write(&in_1);
    writer.write(&in_2);
    writer.write(&in_3);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();
    let out_2 = reader.read().unwrap();
    let out_3 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
}