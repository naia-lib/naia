
pub mod harness;
pub mod test_protocol;
mod builder;
mod endpoint;


pub use harness::Scenario;
pub use test_protocol::{protocol, Position, Auth};
pub use builder::LocalTransportBuilder;
pub use endpoint::{LocalClientEndpoint, LocalServerEndpoint};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
