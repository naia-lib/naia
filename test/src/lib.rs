mod harness;
pub mod test_protocol;

pub use harness::{ClientKey, ExpectCtx, Scenario, ToTicks};
// server events
pub use harness::{
    ServerAuthEvent, ServerConnectEvent, ServerDelegateEntityEvent, ServerDespawnEntityEvent,
    ServerDisconnectEvent, ServerEntityAuthGrantEvent, ServerEntityAuthResetEvent,
    ServerErrorEvent, ServerSpawnEntityEvent, ServerTickEvent,
};
//client events
pub use harness::{
    ClientConnectEvent, ClientDespawnEntityEvent, ClientDisconnectEvent,
    ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent,
    ClientErrorEvent, ClientPublishEntityEvent, ClientRejectEvent, ClientServerTickEvent,
    ClientSpawnEntityEvent, ClientTickEvent, ClientUnpublishEntityEvent,
};
pub use test_protocol::{protocol, Auth, Position};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
