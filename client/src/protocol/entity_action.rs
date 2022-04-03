use naia_shared::{NetEntity, Protocolize};

pub enum EntityAction<P: Protocolize> {
    SpawnEntity(NetEntity, Vec<P>),
    DespawnEntity(NetEntity),
    InsertComponent(NetEntity, P),
    RemoveComponent(NetEntity, P::Kind),
    Noop,
}