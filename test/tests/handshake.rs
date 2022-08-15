use std::time::Duration;

use naia_client::internal::{HandshakeManager as ClientHandshakeManager, HandshakeState};
use naia_server::internal::{HandshakeManager as ServerHandshakeManager, HandshakeResult};
use naia_shared::{
    serde::{BitReader, BitWriter, Serde},
    PacketType, Protocolize, StandardHeader,
};
use naia_test::{Auth, Protocol};

#[test]
fn end_to_end_handshake_w_auth() {
    let mut client = ClientHandshakeManager::<Protocol>::new(Duration::new(0, 0));
    let mut server = ServerHandshakeManager::<Protocol>::new(true);
    let mut message_length: usize;
    let mut message_buffer: [u8; 508];
    let mut writer: BitWriter;
    let mut reader: BitReader;

    // 0. set Client auth object
    let username = "charlie";
    let password = "1234567";
    client.set_auth_message(Protocol::Auth(Auth::new(username, password)));

    // 1. Client send challenge request
    {
        writer = client.write_challenge_request();
        let (length, buffer) = writer.flush();
        message_length = length;
        message_buffer = buffer;
    }

    // 2. Server receive challenge request
    {
        reader = BitReader::new(&message_buffer[..message_length]);
        StandardHeader::de(&mut reader).unwrap();
        writer = server.recv_challenge_request(&mut reader);
    }

    // 3. Server send challenge response
    {
        let (length, buffer) = writer.flush();
        message_length = length;
        message_buffer = buffer;
    }

    // 4. Client receive challenge response
    {
        reader = BitReader::new(&message_buffer[..message_length]);
        StandardHeader::de(&mut reader).unwrap();
        client.recv_challenge_response(&mut reader);
        assert_eq!(
            client.connection_state,
            HandshakeState::AwaitingConnectResponse
        );
    }

    // 5. Client send connect request
    {
        writer = client.write_connect_request();
        let (length, buffer) = writer.flush();
        message_length = length;
        message_buffer = buffer;
    }

    // 6. Server receive connect request
    {
        reader = BitReader::new(&message_buffer[..message_length]);
        StandardHeader::de(&mut reader).unwrap();
        let result = server.recv_connect_request(&mut reader);
        if let HandshakeResult::Success(Some(auth_message)) = result {
            let auth_replica = auth_message
                .cast_ref::<Auth>()
                .expect("did not construct protocol correctly...");
            assert_eq!(
                *auth_replica.username, username,
                "Server received an invalid username: '{}', should be: '{}'",
                *auth_replica.username, username
            );
            assert_eq!(
                *auth_replica.password, password,
                "Server received an invalid password: '{}', should be: '{}'",
                *auth_replica.password, password
            );
        } else {
            assert!(false, "handshake result from server was not correct");
        }
    }

    // 7. Server send connect response
    {
        let header = StandardHeader::new(PacketType::ServerConnectResponse, 0, 0, 0);
        writer = BitWriter::new();
        header.ser(&mut writer);
        let (length, buffer) = writer.flush();
        message_length = length;
        message_buffer = buffer;
    }

    // 8. Client receive connect response
    {
        reader = BitReader::new(&message_buffer[..message_length]);
        StandardHeader::de(&mut reader).unwrap();
        client.recv_connect_response();
    }
}
