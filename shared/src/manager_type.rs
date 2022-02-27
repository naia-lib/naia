use crate::{derive_serde, serde};

// Every data packet transmitted has data specific to either the Message,
// Entity managers. This value is written to differentiate those parts
// of the payload.

#[derive_serde]
pub enum ManagerType {
    Message,
    Entity,
    EntityMessage,
}
