
use super::{
    entity_type::EntityType,
    server_entity_manager::ServerEntityManager,
    client_entity_manager::ClientEntityManager,
};

pub enum EntityManager<U: EntityType> {
    Server(ServerEntityManager<U>),
    Client(ClientEntityManager<U>),
}

