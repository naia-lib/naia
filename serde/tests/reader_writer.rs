use naia_serde::{BitReader, BitWriter};

#[test]
fn read_write_1_bit() {
    let mut writer = BitWriter::new();

    writer.write_bit(true);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(true, reader.read_bit());
}

#[test]
fn read_write_3_bits() {
    let mut writer = BitWriter::new();

    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(true);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(true, reader.read_bit());
}

#[test]
fn read_write_8_bits() {
    let mut writer = BitWriter::new();

    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_bit(false);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
}

#[test]
fn read_write_13_bits() {
    let mut writer = BitWriter::new();

    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_bit(false);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(true);

    writer.write_bit(true);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(true, reader.read_bit());

    assert_eq!(true, reader.read_bit());
}

#[test]
fn read_write_16_bits() {
    let mut writer = BitWriter::new();

    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(false);
    writer.write_bit(false);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(true);

    writer.write_bit(true);
    writer.write_bit(false);
    writer.write_bit(true);
    writer.write_bit(true);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(true, reader.read_bit());

    assert_eq!(true, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true, reader.read_bit());
    assert_eq!(true, reader.read_bit());
}

#[test]
fn read_write_1_byte() {
    let mut writer = BitWriter::new();

    writer.write_byte(123);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(123, reader.read_byte());
}

#[test]
fn read_write_5_bytes() {
    let mut writer = BitWriter::new();

    writer.write_byte(48);
    writer.write_byte(151);
    writer.write_byte(62);
    writer.write_byte(34);
    writer.write_byte(2);

    let (buffer_length, buffer) = writer.flush();

    let mut reader = BitReader::new(buffer_length, buffer);

    assert_eq!(48, reader.read_byte());
    assert_eq!(151, reader.read_byte());
    assert_eq!(62, reader.read_byte());
    assert_eq!(34, reader.read_byte());
    assert_eq!(2, reader.read_byte());
}
