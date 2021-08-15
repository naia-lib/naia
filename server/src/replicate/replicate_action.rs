use naia_shared::{Replicate, ProtocolType, LocalReplicateKey, Ref, DiffMask, LocalEntityKey, EntityKey, LocalComponentKey, ReplicateActionType};

use super::keys::{ObjectKey, ComponentKey};

#[derive(Debug)]
pub enum ReplicateAction<T: ProtocolType> {
    CreateObject(ObjectKey, LocalReplicateKey, Ref<dyn Replicate<T>>),
    UpdateObject(ObjectKey, LocalReplicateKey, Ref<DiffMask>, Ref<dyn Replicate<T>>),
    DeleteObject(ObjectKey, LocalReplicateKey),
    AssignPawn(ObjectKey, LocalReplicateKey),
    UnassignPawn(ObjectKey, LocalReplicateKey),
    UpdatePawn(ObjectKey, LocalReplicateKey, Ref<DiffMask>, Ref<dyn Replicate<T>>),
    CreateEntity(EntityKey, LocalEntityKey, Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn Replicate<T>>)>>),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(LocalEntityKey, ComponentKey, LocalComponentKey, Ref<dyn Replicate<T>>),
}

impl<T: ProtocolType> ReplicateAction<T> {
    pub fn as_type(&self) -> ReplicateActionType {
        match self {
            ReplicateAction::CreateObject(_, _, _) => ReplicateActionType::CreateObject,
            ReplicateAction::DeleteObject(_, _) => ReplicateActionType::DeleteReplicate,
            ReplicateAction::UpdateObject(_, _, _, _) => ReplicateActionType::UpdateReplicate,
            ReplicateAction::AssignPawn(_, _) => ReplicateActionType::AssignPawn,
            ReplicateAction::UnassignPawn(_, _) => ReplicateActionType::UnassignPawn,
            ReplicateAction::UpdatePawn(_, _, _, _) => ReplicateActionType::UpdatePawn,
            ReplicateAction::CreateEntity(_, _, _) => ReplicateActionType::CreateEntity,
            ReplicateAction::DeleteEntity(_, _) => ReplicateActionType::DeleteEntity,
            ReplicateAction::AssignPawnEntity(_, _) => ReplicateActionType::AssignPawnEntity,
            ReplicateAction::UnassignPawnEntity(_, _) => ReplicateActionType::UnassignPawnEntity,
            ReplicateAction::AddComponent(_, _, _, _) => ReplicateActionType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ReplicateAction<T> {
    fn clone(&self) -> Self {
        match self {
            ReplicateAction::CreateObject(gk, lk, e) => {
                ReplicateAction::CreateObject(gk.clone(), lk.clone(), e.clone())
            }
            ReplicateAction::DeleteObject(gk, lk) => {
                ReplicateAction::DeleteObject(gk.clone(), lk.clone())
            }
            ReplicateAction::UpdateObject(gk, lk, sm, e) => {
                ReplicateAction::UpdateObject(gk.clone(), lk.clone(), sm.clone(), e.clone())
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
            ReplicateAction::AddComponent(lek, gck,lck, r) => {
                ReplicateAction::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
