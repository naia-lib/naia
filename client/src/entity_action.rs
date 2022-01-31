use naia_shared::Protocolize;

#[derive(Debug, Clone)]
pub enum EntityAction<P: Protocolize, E: Copy> {
    SpawnEntity(E, Vec<P::Kind>),
    DespawnEntity(E),
    InsertComponent(E, P::Kind),
    UpdateComponent(E, P::Kind),
    RemoveComponent(E, P),
}
