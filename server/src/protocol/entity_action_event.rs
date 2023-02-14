use naia_shared::ComponentKind;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityActionEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, ComponentKind),
}
