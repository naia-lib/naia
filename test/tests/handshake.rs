use std::time::Duration;
use naia_shared::{
    serde::{BitReader, BitWriter, Serde},
    Protocolize,
};

use naia_client::internal::HandshakeManager as ClientHandshakeManager;
use naia_server::internal::HandshakeManager as ServerHandshakeManager;

use naia_test::{Auth, Protocol, ProtocolKind};

#[test]
fn end_to_end_handshake_w_auth() {

    let client = ClientHandshakeManager::<Protocol>::new(Duration::new(0,0));
    let server = ServerHandshakeManager::<Protocol>::new(true);

    // 1. Client send challenge request

    // 2. Server receive challenge request

    // 3. Server send challenge response

    // 4. Client receive challenge response

    // 5. Client send connect request

    // 6. Server receive connect request

    // 7. Server send connect response

    // 8. Client receive connect response

    // Write
    let mut writer = BitWriter::new();

    let username = "charlie".to_string();
    let password = "1234567".to_string();

    let in_1 = Protocol::Auth(Auth::new(username.as_str(), password.as_str()));

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
    assert_eq!(*typed_in_1.username, username);
    assert_eq!(*typed_in_1.password, password);
    assert_eq!(*typed_out_1.username, username);
    assert_eq!(*typed_out_1.password, password);
}
