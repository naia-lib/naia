use crate::ComponentKind;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityActionEvent<E: Copy> {
    SpawnEntity(E, Vec<ComponentKind>),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, ComponentKind),
}
