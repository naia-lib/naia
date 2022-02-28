
use naia_shared::{serde::{BitReader, BitWriter, Serde}, Protocolize};

use naia_test::{Protocol, ProtocolKind, Auth};

#[test]
fn connect_request_flow() {

    // Client send request



    // Write
    let mut writer = BitWriter::new();

    let in_1 = Protocol::Auth(Auth::new("hello world", "goodbye world"));

    in_1.dyn_ref().kind().ser(&mut writer);
    in_1.write(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let manifest = Protocol::load();
    let out_kind = ProtocolKind::de(&mut reader).unwrap();
    let out_1 = manifest.create_replica(out_kind, &mut reader);

    let typed_in_1 = in_1.cast_ref::<Auth>().unwrap();
    let typed_out_1 = out_1.cast_ref::<Auth>().unwrap();
    assert!(typed_in_1.username.equals(&typed_out_1.username));
    assert!(typed_in_1.password.equals(&typed_out_1.password));
    assert_eq!(*typed_in_1.username, "hello world".to_string());
    assert_eq!(*typed_in_1.password, "goodbye world".to_string());
    assert_eq!(*typed_out_1.username, "hello world".to_string());
    assert_eq!(*typed_out_1.password, "goodbye world".to_string());
}
