use naia_shared::Protocolize;

use super::tick::Tick;

#[derive(Debug, Clone)]
pub enum EntityAction<P: Protocolize, E: Copy> {
    SpawnEntity(E, Vec<P::Kind>),
    DespawnEntity(E),
    MessageEntity(E, P),
    InsertComponent(E, P::Kind),
    UpdateComponent(Tick, E, P::Kind),
    RemoveComponent(E, P),
}
