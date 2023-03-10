use std::time::Duration;

use naia_client::internal::{HandshakeManager as ClientHandshakeManager, HandshakeState};
use naia_server::internal::{HandshakeManager as ServerHandshakeManager, HandshakeResult};
use naia_shared::{
    BitReader, BitWriter, FakeEntityConverter, MessageContainer, PacketType, Protocol, Serde,
    StandardHeader,
};
use naia_test::Auth;

#[test]
fn end_to_end_handshake_w_auth() {
    let mut client = ClientHandshakeManager::new(Duration::new(0, 0), Duration::new(0, 0), 1);
    let mut server = ServerHandshakeManager::new(true);
    let mut bytes: Box<[u8]>;
    let mut writer: BitWriter;
    let mut reader: BitReader;

    // Set up Protocol
    let protocol = Protocol::builder().add_message::<Auth>().build();
    let message_kinds = protocol.message_kinds;

    // 0. set Client auth object
    let username = "charlie";
    let password = "1234567";
    client.set_auth_message(MessageContainer::from(
        Box::new(Auth::new(username, password)),
        &FakeEntityConverter,
    ));

    // 1. Client send challenge request
    {
        writer = client.write_challenge_request();
        bytes = writer.to_bytes();
    }

    // 2. Server receive challenge request
    {
        reader = BitReader::new(&bytes);
        StandardHeader::de(&mut reader).expect("unable to read standard header from stream");
        writer = server.recv_challenge_request(&mut reader).unwrap();
    }

    // 3. Server send challenge response
    {
        bytes = writer.to_bytes();
    }

    // 4. Client receive challenge response
    {
        reader = BitReader::new(&bytes);
        StandardHeader::de(&mut reader).expect("unable to read standard header from stream");
        client.recv_challenge_response(&mut reader);
        assert!(client
            .connection_state
            .eq(&HandshakeState::AwaitingValidateResponse));
    }

    // 5. Client send connect request
    {
        writer = client.write_validate_request(&message_kinds);
        bytes = writer.to_bytes();
    }

    // 6. Server receive connect request
    {
        reader = BitReader::new(&bytes);
        StandardHeader::de(&mut reader).expect("unable to read standard header from stream");
        let address = "127.0.0.1:4000".parse().unwrap();
        let result = server.recv_validate_request(&message_kinds, &address, &mut reader);
        if let HandshakeResult::Success(Some(auth_message)) = result {
            let boxed_any = auth_message.to_boxed_any();
            let auth_replica = boxed_any
                .downcast_ref::<Auth>()
                .expect("did not construct protocol correctly...");
            assert_eq!(
                auth_replica.username, username,
                "Server received an invalid username: '{}', should be: '{}'",
                auth_replica.username, username
            );
            assert_eq!(
                auth_replica.password, password,
                "Server received an invalid password: '{}', should be: '{}'",
                auth_replica.password, password
            );
        } else {
            assert!(false, "handshake result from server was not correct");
        }
    }

    // 7. Server send connect response
    {
        let header = StandardHeader::new(PacketType::ServerValidateResponse, 0, 0, 0);
        writer = BitWriter::new();
        header.ser(&mut writer);
        bytes = writer.to_bytes();
    }

    // 8. Client receive connect response
    {
        reader = BitReader::new(&bytes);
        StandardHeader::de(&mut reader).expect("unable to read standard header from stream");
        client.recv_validate_response();
    }
}
