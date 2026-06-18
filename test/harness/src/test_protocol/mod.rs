/// Minimal test protocol for E2E testing
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

// Large request/response pair for fragmentation testing (messaging-16.t3).
// Payloads exceed the ~430-byte MTU so they are fragmented via VecBitWriter +
// ReliableMessageSender::send_or_fragment before transmission.
#[derive(Message)]
pub struct LargeTestRequest {
    pub payload: Vec<u8>,
}

impl LargeTestRequest {
    pub fn new(size: usize) -> Self {
        Self {
            payload: vec![0xABu8; size],
        }
    }
}

#[derive(Message)]
pub struct LargeTestResponse {
    pub payload: Vec<u8>,
}

impl LargeTestResponse {
    pub fn new(size: usize) -> Self {
        Self {
            payload: vec![0xCDu8; size],
        }
    }
}

impl naia_shared::Request for LargeTestRequest {
    type Response = LargeTestResponse;
}

impl naia_shared::Response for LargeTestResponse {}

// Enum message — validates #[derive(Message)] on enums (issue #163).
// Covers all three variant styles: Unit, Named, Unnamed.
#[derive(Message)]
pub enum TestEnumMessage {
    Ping,
    Chat { text: String },
    Move(u32, u32),
}

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

#[derive(Replicate, bevy_ecs::component::Component)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

impl Position {
    pub fn new(x: f32, y: f32) -> Self {
        Self::new_complete(x, y)
    }
}

#[derive(Replicate, bevy_ecs::component::Component)]
pub struct Velocity {
    pub vx: Property<f32>,
    pub vy: Property<f32>,
}

impl Velocity {
    pub fn new(vx: f32, vy: f32) -> Self {
        Self::new_complete(vx, vy)
    }
}

/// Marker component that is replicated immutably.
/// Used by Phase 5 spike tests to verify zero GlobalDiffHandler allocation.
#[derive(Replicate, bevy_ecs::component::Component)]
#[replicate(immutable)]
pub struct ImmutableLabel;

/// Value-carrying immutable (seed-only) component. Its `Property` values are
/// serialized once at spawn/insert to seed each new observer, but it is never
/// diff-tracked — so the host may mutate it every tick without producing
/// updates to existing observers. Exercises the seed-only-replication primitive.
#[derive(Replicate, bevy_ecs::component::Component)]
#[replicate(immutable)]
pub struct ImmutableSeed {
    pub a: Property<u16>,
    pub b: Property<u16>,
}

impl ImmutableSeed {
    pub fn new(a: u16, b: u16) -> Self {
        Self::new_complete(a, b)
    }
}

// ========================================================================
// Replicated Resource test types — server↔client singletons.
// ========================================================================

/// Server-authoritative scoreboard resource. Used by integration tests
/// to assert end-to-end resource replication and per-field diff updates.
#[derive(Replicate, bevy_ecs::resource::Resource, bevy_ecs::component::Component)]
pub struct TestScore {
    pub home: Property<u32>,
    pub away: Property<u32>,
}

impl TestScore {
    pub fn new(home: u32, away: u32) -> Self {
        Self::new_complete(home, away)
    }
}

/// Server-authoritative match-state resource (used by static-pool tests).
#[derive(Replicate, bevy_ecs::resource::Resource, bevy_ecs::component::Component)]
pub struct TestMatchState {
    pub phase: Property<u8>,
}

impl TestMatchState {
    pub fn new(phase: u8) -> Self {
        Self::new_complete(phase)
    }
}

/// Delegable resource — registered as a normal resource; the
/// authority-delegation tests configure delegation at insert time via
/// `configure_replicated_resource`.
#[derive(Replicate, bevy_ecs::resource::Resource, bevy_ecs::component::Component)]
pub struct TestPlayerSelection {
    pub selected_id: Property<u16>,
}

impl TestPlayerSelection {
    pub fn new(selected_id: u16) -> Self {
        Self::new_complete(selected_id)
    }
}

pub fn protocol() -> Protocol {
    Protocol::builder()
        .add_component::<Position>()
        .add_component::<Velocity>()
        .add_component::<ImmutableLabel>()
        .add_component::<ImmutableSeed>()
        .add_resource::<TestScore>()
        .add_resource::<TestMatchState>()
        .add_resource::<TestPlayerSelection>()
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

/// Extended protocol that adds `LargeTestRequest`/`LargeTestResponse` for the
/// request/response fragmentation test (`messaging_16b`).  Kept separate from
/// `protocol()` so the main protocol's fingerprint is stable and the mismatch
/// detection tests are unaffected.
pub fn protocol_with_large_req_resp() -> Protocol {
    Protocol::builder()
        .add_component::<Position>()
        .add_component::<Velocity>()
        .add_component::<ImmutableLabel>()
        .add_component::<ImmutableSeed>()
        .add_resource::<TestScore>()
        .add_resource::<TestMatchState>()
        .add_resource::<TestPlayerSelection>()
        .add_message::<Auth>()
        .add_message::<TestMessage>()
        .add_message::<LargeTestMessage>()
        .add_message::<EntityCommandMessage>()
        .add_message::<TestRequest>()
        .add_message::<TestResponse>()
        .add_message::<LargeTestRequest>()
        .add_message::<LargeTestResponse>()
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

#[cfg(test)]
mod immutable_seed_tests {
    //! Value-carrying immutable (seed-only) replication primitive.
    //!
    //! An immutable component carries its `Property` values once at spawn/insert
    //! (to seed each new observer) but is never diff-tracked. These tests pin the
    //! two load-bearing behaviors: the value survives the spawn write→read
    //! round-trip, and the host may mutate the component every tick despite never
    //! receiving a `PropertyMutator`.

    use super::{protocol, ImmutableSeed};
    use naia_shared::{FakeEntityConverter, Replicate};

    #[test]
    fn immutable_seed_is_flagged_immutable() {
        let seed = ImmutableSeed::new(220, 75);
        assert!(
            seed.is_immutable(),
            "a #[replicate(immutable)] component with Property fields must still report is_immutable()"
        );
    }

    #[test]
    fn seed_value_round_trips_through_spawn_write() {
        // Serialize EXACTLY as SpawnWithComponents does (Replicate::write =
        // kind tag + each Property value), then read it back through the
        // protocol's component reader — proving the seed value crosses the wire
        // rather than arriving as Default (the bug that sank the immutable
        // presence-only approach).
        let protocol = protocol();
        let kinds = &protocol.component_kinds;

        let seed = ImmutableSeed::new(220, 75);
        let mut writer = naia_shared::BitWriter::new();
        let mut write_converter = FakeEntityConverter;
        seed.write(kinds, &mut writer, &mut write_converter);

        let owned = writer.to_owned_reader();
        let mut reader = owned.borrow();
        let boxed = kinds
            .read(&mut reader, &FakeEntityConverter)
            .expect("immutable seed component should deserialize");

        let read_back = boxed
            .to_any()
            .downcast_ref::<ImmutableSeed>()
            .expect("read-back component should be an ImmutableSeed");
        assert_eq!(*read_back.a, 220, "seed field `a` must survive the wire");
        assert_eq!(*read_back.b, 75, "seed field `b` must survive the wire");
    }

    #[test]
    fn host_may_mutate_every_tick_without_a_mutator() {
        // Immutable components are never registered for diff-tracking, so they
        // never receive a PropertyMutator. The host sim mutates the live value
        // every tick (each new observer is seeded with the current value at
        // spawn). This must be a clean no-op on dirty-tracking — no panic, no
        // warn spam — and the value must update in place.
        let mut seed = ImmutableSeed::new(0, 0);
        for i in 1..=100u16 {
            *seed.a = i;
            *seed.b = i.wrapping_mul(2);
        }
        assert_eq!(*seed.a, 100);
        assert_eq!(*seed.b, 200);
    }
}
