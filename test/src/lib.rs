
mod harness;
mod test_protocol;

pub use harness::{Scenario, ClientKey};
pub use test_protocol::{protocol, Auth, Position};

// Re-export demo_world types for tests
pub use naia_demo_world::{Entity as TestEntity, World as TestWorld};
