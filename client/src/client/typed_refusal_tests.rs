use naia_shared::{
    Channel, ChannelDirection, ChannelMode, Message, Protocol, ReliableSettings, Request, Response,
};

use super::*;
use crate::NaiaClientError;

#[derive(Channel)]
struct TestRequestChannel;

#[derive(Message)]
struct TestRequest {
    query: u32,
}

#[derive(Message)]
struct TestResponse {
    result: u32,
}

impl Request for TestRequest {
    type Response = TestResponse;
}

impl Response for TestResponse {}

#[derive(Channel)]
struct TestCappedChannel;

#[derive(Message)]
struct TestCappedMessage {
    text: u32,
}

#[test]
fn send_request_before_connect_returns_not_connected() {
    let mut proto = Protocol::builder();
    proto
        .add_channel::<TestRequestChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_message::<TestRequest>()
        .add_message::<TestResponse>();
    // Unlocked: Client::new locks the protocol itself.
    let protocol = proto.build();

    let mut client = Client::<u64>::new(ClientConfig::default(), protocol);
    let request = TestRequest { query: 7 };
    assert!(matches!(
        client.send_request::<TestRequestChannel, _>(&request),
        Err(NaiaClientError::NotConnected)
    ));
}

/// Census 29818 Hit A: the pre-connect waitlist is the queue, so the
/// channel's `max_queue_depth` must apply to it exactly as it does on
/// the live path. Otherwise disconnected submits bypass the documented
/// `MessageQueueFull` backpressure without bound (and the MVP-path chat
/// must track transport failure in parallel because every submit
/// answers Ok).
#[test]
fn send_message_before_connect_refuses_past_queue_cap() {
    let mut proto = Protocol::builder();
    proto
        .add_channel::<TestCappedChannel>(
            ChannelDirection::ClientToServer,
            ChannelMode::UnorderedReliable(ReliableSettings {
                max_queue_depth: Some(2),
                ..ReliableSettings::default()
            }),
        )
        .add_message::<TestCappedMessage>();
    let protocol = proto.build();

    let mut client = Client::<u64>::new(ClientConfig::default(), protocol);
    let message = TestCappedMessage { text: 1 };
    assert!(client
        .send_message::<TestCappedChannel, _>(&message)
        .is_ok());
    assert!(client
        .send_message::<TestCappedChannel, _>(&message)
        .is_ok());
    assert!(matches!(
        client.send_message::<TestCappedChannel, _>(&message),
        Err(NaiaClientError::MessageQueueFull)
    ));
}
