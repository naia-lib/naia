use naia_shared::ProtocolType;

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType, E: Copy> {
    SpawnEntity(E, Vec<P::Kind>),
    DespawnEntity(E),
    InsertComponent(E, P::Kind),
    UpdateComponent(E, P::Kind),
    RemoveComponent(E, P),
}
