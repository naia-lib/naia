mod harness;
pub mod scenarios;
pub mod test_protocol;

pub use harness::{
    ClientExpectCtx, ClientKey, DiffHandlerSnapshot, EntityKey, EntityOwner, ExpectCtx,
    ExpectResult, OperationResult, Scenario, ServerExpectCtx, Trace, TraceDirection, TraceEvent,
    TracePacket, ToTicks, TrackedClientEvent, TrackedServerEvent,
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
pub use test_protocol::{protocol, Auth, EntityCommandMessage, ImmutableLabel, LargeTestMessage, Position, Velocity};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
