use byteorder::{BigEndian, WriteBytesExt};

use super::{
    entity_type::EntityType,
    server_entity_manager::ServerEntityManager,
    client_entity_manager::ClientEntityManager,
};

use slotmap::{new_key_type};

pub type LocalEntityKey = u16;

pub trait LocalEntityKeyIO {
    fn write(&self, out_bytes: &mut Vec<u8>);
}

impl LocalEntityKeyIO for &LocalEntityKey {
    fn write(&self, out_bytes: &mut Vec<u8>) {
        out_bytes.write_u16::<BigEndian>(**self);
    }
}

pub enum EntityManager<U: EntityType> {
    Server(ServerEntityManager<U>),
    Client(ClientEntityManager<U>),
}

