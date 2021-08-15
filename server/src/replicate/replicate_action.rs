use naia_shared::{
    DiffMask, EntityKey, LocalComponentKey, LocalEntityKey, LocalObjectKey, LocalReplicaKey,
    ProtocolType, Ref, ReplicaActionType, Replicate,
};

use super::keys::{ComponentKey, ObjectKey};

#[derive(Debug)]
pub enum ReplicateAction<T: ProtocolType> {
    CreateObject(ObjectKey, LocalObjectKey, Ref<dyn Replicate<T>>),
    UpdateReplicate(
        ObjectKey,
        LocalReplicaKey,
        Ref<DiffMask>,
        Ref<dyn Replicate<T>>,
    ),
    DeleteReplicate(ObjectKey, LocalReplicaKey),
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

impl<T: ProtocolType> ReplicateAction<T> {
    pub fn as_type(&self) -> ReplicaActionType {
        match self {
            ReplicateAction::CreateObject(_, _, _) => ReplicaActionType::CreateObject,
            ReplicateAction::DeleteReplicate(_, _) => ReplicaActionType::DeleteReplica,
            ReplicateAction::UpdateReplicate(_, _, _, _) => ReplicaActionType::UpdateReplica,
            ReplicateAction::AssignPawn(_, _) => ReplicaActionType::AssignPawn,
            ReplicateAction::UnassignPawn(_, _) => ReplicaActionType::UnassignPawn,
            ReplicateAction::UpdatePawn(_, _, _, _) => ReplicaActionType::UpdatePawn,
            ReplicateAction::CreateEntity(_, _, _) => ReplicaActionType::CreateEntity,
            ReplicateAction::DeleteEntity(_, _) => ReplicaActionType::DeleteEntity,
            ReplicateAction::AssignPawnEntity(_, _) => ReplicaActionType::AssignPawnEntity,
            ReplicateAction::UnassignPawnEntity(_, _) => ReplicaActionType::UnassignPawnEntity,
            ReplicateAction::AddComponent(_, _, _, _) => ReplicaActionType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ReplicateAction<T> {
    fn clone(&self) -> Self {
        match self {
            ReplicateAction::CreateObject(gk, lk, e) => {
                ReplicateAction::CreateObject(gk.clone(), lk.clone(), e.clone())
            }
            ReplicateAction::DeleteReplicate(gk, lk) => {
                ReplicateAction::DeleteReplicate(gk.clone(), lk.clone())
            }
            ReplicateAction::UpdateReplicate(gk, lk, sm, e) => {
                ReplicateAction::UpdateReplicate(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicateAction::AssignPawn(gk, lk) => {
                ReplicateAction::AssignPawn(gk.clone(), lk.clone())
            }
            ReplicateAction::UnassignPawn(gk, lk) => {
                ReplicateAction::UnassignPawn(gk.clone(), lk.clone())
            }
            ReplicateAction::UpdatePawn(gk, lk, sm, e) => {
                ReplicateAction::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicateAction::CreateEntity(gk, lk, cs) => {
                ReplicateAction::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ReplicateAction::DeleteEntity(gk, lk) => {
                ReplicateAction::DeleteEntity(gk.clone(), lk.clone())
            }
            ReplicateAction::AssignPawnEntity(gk, lk) => {
                ReplicateAction::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicateAction::UnassignPawnEntity(gk, lk) => {
                ReplicateAction::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicateAction::AddComponent(lek, gck, lck, r) => {
                ReplicateAction::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
