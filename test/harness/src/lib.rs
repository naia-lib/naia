mod harness;
pub mod scenarios;
pub mod test_protocol;

pub use harness::{
    ClientExpectCtx, ClientKey, DiffHandlerSnapshot, EntityKey, EntityOwner, ExpectCtx,
    ExpectResult, OperationResult, Scenario, ServerExpectCtx, ToTicks, Trace, TraceDirection,
    TraceEvent, TracePacket, TrackedClientEvent, TrackedServerEvent,
};
pub use naia_shared::handshake::RejectReason;
pub use naia_shared::LinkConditionerConfig;
pub use naia_shared::ProtocolId;
// server events
pub use harness::{
    ServerAuthEvent, ServerConnectEvent, ServerDelegateEntityEvent, ServerDespawnEntityEvent,
    ServerDisconnectEvent, ServerEntityAuthGrantEvent, ServerEntityAuthResetEvent,
    ServerErrorEvent, ServerPublishEntityEvent, ServerSpawnEntityEvent, ServerTickEvent,
    ServerUnpublishEntityEvent,
};
//client events
pub use harness::{
    ClientConnectEvent, ClientDespawnEntityEvent, ClientDisconnectEvent,
    ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent,
    ClientErrorEvent, ClientPublishEntityEvent, ClientRejectEvent, ClientServerTickEvent,
    ClientSpawnEntityEvent, ClientTickEvent, ClientUnpublishEntityEvent,
};
pub use test_protocol::{
    protocol, protocol_with_large_req_resp, Auth, EntityCommandMessage, ImmutableLabel,
    LargeTestMessage, LargeTestRequest, LargeTestResponse, Position, TestMatchState,
    TestPlayerSelection, TestScore, Velocity,
};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
