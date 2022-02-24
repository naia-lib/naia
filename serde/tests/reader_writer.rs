
use naia_serde::{BitReader, BitWriter};

#[test]
fn read_write_1_bits() {
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
    assert_eq!(true,  reader.read_bit());
    assert_eq!(true,  reader.read_bit());
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
    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());

    assert_eq!(true,  reader.read_bit());
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
    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());

    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());

    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());
    assert_eq!(true,  reader.read_bit());

    assert_eq!(true,  reader.read_bit());
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
    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());

    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(false, reader.read_bit());

    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());
    assert_eq!(true,  reader.read_bit());

    assert_eq!(true,  reader.read_bit());
    assert_eq!(false, reader.read_bit());
    assert_eq!(true,  reader.read_bit());
    assert_eq!(true,  reader.read_bit());
}