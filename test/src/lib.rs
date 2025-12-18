
mod harness;
pub mod test_protocol;

pub use harness::{Scenario, ClientKey, AuthEvent, ConnectEvent, ServerDisconnectEvent, ClientDisconnectEvent, RejectEvent, ClientConnectEvent, EntityAuthGrantEvent, EntityAuthResetEvent as ServerEntityAuthResetEvent, DelegateEntityEvent, EntityAuthGrantedEvent, ClientEntityAuthResetEvent, EntityAuthDeniedEvent, ServerTickEvent, ClientTickEvent, ClientServerTickEvent};
pub use test_protocol::{protocol, Auth, Position};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
