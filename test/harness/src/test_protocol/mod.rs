/// Minimal test protocol for E2E testing
use bevy_ecs::prelude::Component;

use naia_shared::{
    Channel, ChannelDirection, ChannelMode, EntityProperty, Message, Property, Protocol, 
    ReliableSettings, Replicate, TickBufferSettings,
};

#[derive(Message, PartialEq, Eq, Hash)]
pub struct Auth {
    pub username: String,
    pub password: String,
}

impl Auth {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
        }
    }
}

#[derive(Message, PartialEq, Eq, Hash)]
pub struct TestMessage {
    pub value: u32,
}

impl TestMessage {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
}

// Large message for fragmentation testing (messaging-15, messaging-16)
#[derive(Message)]
pub struct LargeTestMessage {
    pub payload: Vec<u8>,
}

impl LargeTestMessage {
    pub fn new(size: usize) -> Self {
        Self {
            payload: vec![0u8; size],
        }
    }
}

// Message with EntityProperty for buffering tests (messaging-18, messaging-19, messaging-20)
#[derive(Message)]
pub struct EntityCommandMessage {
    pub target: EntityProperty,
    pub command: String,
}

impl EntityCommandMessage {
    pub fn new(command: &str) -> Self {
        Self {
            target: EntityProperty::new_for_message(),
            command: command.to_string(),
        }
    }
}

#[derive(Message, PartialEq, Eq, Hash)]
pub struct TestRequest {
    pub query: String,
}

impl TestRequest {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
        }
    }
}

#[derive(Message, PartialEq, Eq, Hash)]
pub struct TestResponse {
    pub result: String,
}

impl TestResponse {
    pub fn new(result: &str) -> Self {
        Self {
            result: result.to_string(),
        }
    }
}

impl naia_shared::Request for TestRequest {
    type Response = TestResponse;
}

impl naia_shared::Response for TestResponse {}

// Channels for testing
#[derive(Channel)]
pub struct ReliableChannel;

#[derive(Channel)]
pub struct UnreliableChannel;

#[derive(Channel)]
pub struct OrderedChannel;

#[derive(Channel)]
pub struct UnorderedChannel;

#[derive(Channel)]
pub struct SequencedChannel;

#[derive(Channel)]
pub struct TickBufferedChannel;

#[derive(Channel)]
pub struct RequestResponseChannel;

/// Server-to-client only channel for testing direction enforcement
#[derive(Channel)]
pub struct ServerToClientChannel;

#[derive(Component, Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

impl Position {
    pub fn new(x: f32, y: f32) -> Self {
        Self::new_complete(x, y)
    }
}

pub fn protocol() -> Protocol {
    Protocol::builder()
        .add_component::<Position>()
        .add_message::<Auth>()
        .add_message::<TestMessage>()
        .add_message::<LargeTestMessage>()
        .add_message::<EntityCommandMessage>()
        .add_message::<TestRequest>()
        .add_message::<TestResponse>()
        .add_channel::<ReliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_channel::<UnreliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedUnreliable,
        )
        .add_channel::<OrderedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::OrderedReliable(ReliableSettings::default()),
        )
        .add_channel::<UnorderedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_channel::<SequencedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::SequencedReliable(ReliableSettings::default()),
        )
        .add_channel::<TickBufferedChannel>(
            ChannelDirection::ClientToServer,
            ChannelMode::TickBuffered(TickBufferSettings::default()),
        )
        .add_channel::<RequestResponseChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_channel::<ServerToClientChannel>(
            ChannelDirection::ServerToClient,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .enable_client_authoritative_entities()
        .build()
}
