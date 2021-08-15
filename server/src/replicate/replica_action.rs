use naia_shared::{
    DiffMask, EntityKey, LocalComponentKey, LocalEntityKey, LocalObjectKey, LocalReplicaKey,
    ProtocolType, Ref, ReplicaActionType, Replicate,
};

use super::keys::{ComponentKey, ObjectKey};

#[derive(Debug)]
pub enum ReplicaAction<T: ProtocolType> {
    CreateObject(ObjectKey, LocalObjectKey, Ref<dyn Replicate<T>>),
    UpdateReplica(
        ObjectKey,
        LocalReplicaKey,
        Ref<DiffMask>,
        Ref<dyn Replicate<T>>,
    ),
    DeleteReplica(ObjectKey, LocalReplicaKey),
    AssignPawn(ObjectKey, LocalObjectKey),
    UnassignPawn(ObjectKey, LocalObjectKey),
    UpdatePawn(
        ObjectKey,
        LocalObjectKey,
        Ref<DiffMask>,
        Ref<dyn Replicate<T>>,
    ),
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
            ReplicaAction::CreateObject(_, _, _) => ReplicaActionType::CreateObject,
            ReplicaAction::DeleteReplica(_, _) => ReplicaActionType::DeleteReplica,
            ReplicaAction::UpdateReplica(_, _, _, _) => ReplicaActionType::UpdateReplica,
            ReplicaAction::AssignPawn(_, _) => ReplicaActionType::AssignPawn,
            ReplicaAction::UnassignPawn(_, _) => ReplicaActionType::UnassignPawn,
            ReplicaAction::UpdatePawn(_, _, _, _) => ReplicaActionType::UpdatePawn,
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
            ReplicaAction::CreateObject(gk, lk, e) => {
                ReplicaAction::CreateObject(gk.clone(), lk.clone(), e.clone())
            }
            ReplicaAction::DeleteReplica(gk, lk) => {
                ReplicaAction::DeleteReplica(gk.clone(), lk.clone())
            }
            ReplicaAction::UpdateReplica(gk, lk, sm, e) => {
                ReplicaAction::UpdateReplica(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicaAction::AssignPawn(gk, lk) => {
                ReplicaAction::AssignPawn(gk.clone(), lk.clone())
            }
            ReplicaAction::UnassignPawn(gk, lk) => {
                ReplicaAction::UnassignPawn(gk.clone(), lk.clone())
            }
            ReplicaAction::UpdatePawn(gk, lk, sm, e) => {
                ReplicaAction::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
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
