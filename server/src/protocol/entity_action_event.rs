use naia_shared::ComponentId;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityActionEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentId),
    RemoveComponent(E, ComponentId),
}
