
mod harness;
pub mod test_protocol;

pub use harness::{Scenario, ClientKey, AuthEvent, ConnectEvent, ServerDisconnectEvent, ClientDisconnectEvent, RejectEvent, ClientConnectEvent, EntityAuthGrantEvent, EntityAuthResetEvent as ServerEntityAuthResetEvent, DelegateEntityEvent, EntityAuthGrantedEvent, ClientEntityAuthResetEvent, EntityAuthDeniedEvent};
pub use harness::client_events::{ClientTickEvent, ServerTickEvent as ClientServerTickEvent};
pub use harness::server_events::TickEvent as ServerTickEvent;
pub use test_protocol::{protocol, Auth, Position};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
