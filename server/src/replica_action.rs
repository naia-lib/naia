use naia_shared::{
    DiffMask, EntityKey, LocalComponentKey, LocalEntityKey,
    ProtocolType, Ref, ReplicaActionType, Replicate,
};

use super::keys::component_key::ComponentKey;

#[derive(Debug)]
pub enum ReplicaAction<T: ProtocolType> {
    UpdateReplica(
    ComponentKey,
    LocalComponentKey,
    Ref<DiffMask>,
    Ref<dyn Replicate<T>>,
    ),
    DeleteReplica(ComponentKey, LocalComponentKey),
    CreateEntity(
        EntityKey,
        LocalEntityKey,
        Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn Replicate<T>>)>>,
    ),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(
        LocalEntityKey,
        ComponentKey,
        LocalComponentKey,
        Ref<dyn Replicate<T>>,
    ),
}

impl<T: ProtocolType> ReplicaAction<T> {
    pub fn as_type(&self) -> ReplicaActionType {
        match self {
            ReplicaAction::DeleteReplica(_, _) => ReplicaActionType::DeleteReplica,
            ReplicaAction::UpdateReplica(_, _, _, _) => ReplicaActionType::UpdateReplica,
            ReplicaAction::CreateEntity(_, _, _) => ReplicaActionType::CreateEntity,
            ReplicaAction::DeleteEntity(_, _) => ReplicaActionType::DeleteEntity,
            ReplicaAction::AssignPawnEntity(_, _) => ReplicaActionType::AssignPawnEntity,
            ReplicaAction::UnassignPawnEntity(_, _) => ReplicaActionType::UnassignPawnEntity,
            ReplicaAction::AddComponent(_, _, _, _) => ReplicaActionType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ReplicaAction<T> {
    fn clone(&self) -> Self {
        match self {
            ReplicaAction::DeleteReplica(gk, lk) => {
                ReplicaAction::DeleteReplica(gk.clone(), lk.clone())
            }
            ReplicaAction::UpdateReplica(gk, lk, sm, e) => {
                ReplicaAction::UpdateReplica(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicaAction::CreateEntity(gk, lk, cs) => {
                ReplicaAction::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ReplicaAction::DeleteEntity(gk, lk) => {
                ReplicaAction::DeleteEntity(gk.clone(), lk.clone())
            }
            ReplicaAction::AssignPawnEntity(gk, lk) => {
                ReplicaAction::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicaAction::UnassignPawnEntity(gk, lk) => {
                ReplicaAction::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicaAction::AddComponent(lek, gck, lck, r) => {
                ReplicaAction::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
