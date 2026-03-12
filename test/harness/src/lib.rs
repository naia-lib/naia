mod harness;
pub mod test_protocol;

pub use harness::{
    ClientExpectCtx, ClientKey, EntityKey, EntityOwner, ExpectCtx, ExpectResult, OperationResult,
    Scenario, ServerExpectCtx, ToTicks, TraceEvent, TrackedClientEvent, TrackedServerEvent,
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
pub use test_protocol::{protocol, Auth, EntityCommandMessage, LargeTestMessage, Position};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
