use naia_shared::{Replicate, ProtocolType, LocalObjectKey, Ref, DiffMask, LocalEntityKey, EntityKey, LocalComponentKey, ReplicateMessageType};

use super::object_key::{object_key::ObjectKey, ComponentKey};

#[derive(Debug)]
pub enum ReplicateMessage<T: ProtocolType> {
    CreateReplicate(ObjectKey, LocalObjectKey, Ref<dyn Replicate<T>>),
    UpdateReplicate(ObjectKey, LocalObjectKey, Ref<DiffMask>, Ref<dyn Replicate<T>>),
    DeleteReplicate(ObjectKey, LocalObjectKey),
    AssignPawn(ObjectKey, LocalObjectKey),
    UnassignPawn(ObjectKey, LocalObjectKey),
    UpdatePawn(ObjectKey, LocalObjectKey, Ref<DiffMask>, Ref<dyn Replicate<T>>),
    CreateEntity(EntityKey, LocalEntityKey, Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn Replicate<T>>)>>),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(LocalEntityKey, ComponentKey, LocalComponentKey, Ref<dyn Replicate<T>>),
}

impl<T: ProtocolType> ReplicateMessage<T> {
    pub fn as_type(&self) -> ReplicateMessageType {
        match self {
            ReplicateMessage::CreateReplicate(_, _, _) => ReplicateMessageType::CreateReplicate,
            ReplicateMessage::DeleteReplicate(_, _) => ReplicateMessageType::DeleteReplicate,
            ReplicateMessage::UpdateReplicate(_, _, _, _) => ReplicateMessageType::UpdateReplicate,
            ReplicateMessage::AssignPawn(_, _) => ReplicateMessageType::AssignPawn,
            ReplicateMessage::UnassignPawn(_, _) => ReplicateMessageType::UnassignPawn,
            ReplicateMessage::UpdatePawn(_, _, _, _) => ReplicateMessageType::UpdatePawn,
            ReplicateMessage::CreateEntity(_, _, _) => ReplicateMessageType::CreateEntity,
            ReplicateMessage::DeleteEntity(_, _) => ReplicateMessageType::DeleteEntity,
            ReplicateMessage::AssignPawnEntity(_, _) => ReplicateMessageType::AssignPawnEntity,
            ReplicateMessage::UnassignPawnEntity(_, _) => ReplicateMessageType::UnassignPawnEntity,
            ReplicateMessage::AddComponent(_, _, _, _) => ReplicateMessageType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ReplicateMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ReplicateMessage::CreateReplicate(gk, lk, e) => {
                ReplicateMessage::CreateReplicate(gk.clone(), lk.clone(), e.clone())
            }
            ReplicateMessage::DeleteReplicate(gk, lk) => {
                ReplicateMessage::DeleteReplicate(gk.clone(), lk.clone())
            }
            ReplicateMessage::UpdateReplicate(gk, lk, sm, e) => {
                ReplicateMessage::UpdateReplicate(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicateMessage::AssignPawn(gk, lk) => {
                ReplicateMessage::AssignPawn(gk.clone(), lk.clone())
            }
            ReplicateMessage::UnassignPawn(gk, lk) => {
                ReplicateMessage::UnassignPawn(gk.clone(), lk.clone())
            }
            ReplicateMessage::UpdatePawn(gk, lk, sm, e) => {
                ReplicateMessage::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ReplicateMessage::CreateEntity(gk, lk, cs) => {
                ReplicateMessage::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ReplicateMessage::DeleteEntity(gk, lk) => {
                ReplicateMessage::DeleteEntity(gk.clone(), lk.clone())
            }
            ReplicateMessage::AssignPawnEntity(gk, lk) => {
                ReplicateMessage::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicateMessage::UnassignPawnEntity(gk, lk) => {
                ReplicateMessage::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ReplicateMessage::AddComponent(lek, gck,lck, r) => {
                ReplicateMessage::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
