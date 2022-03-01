use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, time::Duration};
use naia_shared::{serde::{BitReader, BitWriter, Serde}, Protocolize, StandardHeader, PacketType, Manifest, Timestamp};

use naia_client::internal::{HandshakeManager as ClientHandshakeManager, HandshakeState};
use naia_server::internal::{HandshakeManager as ServerHandshakeManager, HandshakeResult};

use naia_test::{Auth, Protocol, ProtocolKind};

#[test]
fn end_to_end_handshake_w_auth() {

    let mut client = ClientHandshakeManager::<Protocol>::new(Duration::new(0,0));
    let mut server = ServerHandshakeManager::<Protocol>::new(true);
    let mut message_length: usize;
    let mut message_buffer: [u8; 508];
    let mut writer: BitWriter;
    let mut reader: BitReader;

    let test_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);;

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
        assert_eq!(client.connection_state, HandshakeState::AwaitingConnectResponse);
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
        let result = server.recv_new_connect_request(&Protocol::load(), &test_socket_addr, &mut reader);
        if let HandshakeResult::AuthUser(auth_message) = result {
            let auth_replica = auth_message.cast_ref::<Auth>().expect("did not construct protocol correctly...");
            assert_eq!(*auth_replica.username,
                       username, "Server received an invalid username: '{}', should be: '{}'",
                       *auth_replica.username,
                       username);
            assert_eq!(*auth_replica.password,
                       password, "Server received an invalid password: '{}', should be: '{}'",
                       *auth_replica.password,
                       password);
        } else {
            assert!(false, "handshake result from server was not correct");
        }
    }

    // 7. Server send connect response
    {
        let header = StandardHeader::new(PacketType::ServerConnectResponse, 0, 0, 0, 0);
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

#[test]
fn connect_request() {

    let mut message_length: usize;
    let mut message_buffer: [u8; 508];
    let mut writer: BitWriter;
    let mut reader: BitReader;

    let test_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);;

    // setup client
    let username = "charlie";
    let password = "1234567";

    // 1. Client send connect request
    {
        writer = client_write_connect_request(&Protocol::Auth(Auth::new(username, password)));
        let (length, buffer) = writer.flush();
        message_length = length;
        message_buffer = buffer;
    }

    // 2. Server receive connect request
    {
        reader = BitReader::new(&message_buffer[..message_length]);
        StandardHeader::de(&mut reader).unwrap();
        let result = server_recv_connect_request(&Protocol::load(), &test_socket_addr, &mut reader);
        if let HandshakeResult::AuthUser(auth_message) = result {
            let auth_replica = auth_message.cast_ref::<Auth>().expect("did not construct protocol correctly...");
            assert_eq!(*auth_replica.username,
                       username, "Server received an invalid username: '{}', should be: '{}'",
                       *auth_replica.username,
                       username);
            assert_eq!(*auth_replica.password,
                       password, "Server received an invalid password: '{}', should be: '{}'",
                       *auth_replica.password,
                       password);
        } else {
            assert!(false, "handshake result from server was not correct");
        }
    }
}

pub fn client_write_connect_request(auth_message: &Protocol) -> BitWriter {
    let mut writer = BitWriter::new();

    StandardHeader::new(PacketType::ClientConnectRequest, 0, 0, 0, 0)
        .ser(&mut writer);

    // write timestamp & digest into payload
    write_signed_timestamp(&mut writer);

    // write auth message if there is one

    // write that we have auth
    1.ser(&mut writer);
    // write auth kind
    auth_message.dyn_ref().kind().ser(&mut writer);
    // write payload
    auth_message.write(&mut writer);

    writer
}

fn write_signed_timestamp(writer: &mut BitWriter) {
    let no_u64 = 0 as u64;
    no_u64
        .ser(writer);

    for _ in 0..32 {
        let digest_byte = 0 as u8;
        digest_byte.ser(writer);
    }
}

pub fn server_recv_connect_request(manifest: &Manifest<Protocol>, _addr: &SocketAddr, reader: &mut BitReader) -> HandshakeResult<Protocol> {
    let _timestamp= timestamp_validate(reader);
    let _has_auth = u8::de(reader).unwrap() == 1;

    let auth_kind = ProtocolKind::de(reader).unwrap();
    let auth_message = manifest.create_replica(auth_kind, reader);
    return HandshakeResult::AuthUser(auth_message);
}

fn timestamp_validate(reader: &mut BitReader) -> Timestamp {
    let timestamp = u64::de(reader).unwrap();
    let mut digest_bytes: Vec<u8> = Vec::new();
    for _ in 0..32 {
        digest_bytes.push(u8::de(reader).unwrap());
    }

    return Timestamp::from_u64(&timestamp);
}