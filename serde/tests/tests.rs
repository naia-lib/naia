
use naia_serde::{BitReader, BitWriter};

#[test]
fn writer_reader_in_out() {
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