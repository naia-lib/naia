use super::{
    entity_type::EntityType,
    server_entity_manager::ServerEntityManager,
    client_entity_manager::ClientEntityManager,
};

use slotmap::{new_key_type};

new_key_type! { pub struct LocalEntityKey; }

pub enum EntityManager<U: EntityType> {
    Server(ServerEntityManager<U>),
    Client(ClientEntityManager<U>),
}

