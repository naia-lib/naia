use naia_shared::{Replicate, ProtocolType, LocalObjectKey, Ref, DiffMask, LocalEntityKey, EntityKey, LocalComponentKey, ReplicateMessageType};

use super::object_key::{object_key::ObjectKey, ComponentKey};

#[derive(Debug)]
pub enum ServerReplicateMessage<T: ProtocolType> {
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

impl<T: ProtocolType> ServerReplicateMessage<T> {
    pub fn as_type(&self) -> ReplicateMessageType {
        match self {
            ServerReplicateMessage::CreateReplicate(_, _, _) => ReplicateMessageType::CreateReplicate,
            ServerReplicateMessage::DeleteReplicate(_, _) => ReplicateMessageType::DeleteReplicate,
            ServerReplicateMessage::UpdateReplicate(_, _, _, _) => ReplicateMessageType::UpdateReplicate,
            ServerReplicateMessage::AssignPawn(_, _) => ReplicateMessageType::AssignPawn,
            ServerReplicateMessage::UnassignPawn(_, _) => ReplicateMessageType::UnassignPawn,
            ServerReplicateMessage::UpdatePawn(_, _, _, _) => ReplicateMessageType::UpdatePawn,
            ServerReplicateMessage::CreateEntity(_, _, _) => ReplicateMessageType::CreateEntity,
            ServerReplicateMessage::DeleteEntity(_, _) => ReplicateMessageType::DeleteEntity,
            ServerReplicateMessage::AssignPawnEntity(_, _) => ReplicateMessageType::AssignPawnEntity,
            ServerReplicateMessage::UnassignPawnEntity(_, _) => ReplicateMessageType::UnassignPawnEntity,
            ServerReplicateMessage::AddComponent(_, _, _, _) => ReplicateMessageType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ServerReplicateMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerReplicateMessage::CreateReplicate(gk, lk, e) => {
                ServerReplicateMessage::CreateReplicate(gk.clone(), lk.clone(), e.clone())
            }
            ServerReplicateMessage::DeleteReplicate(gk, lk) => {
                ServerReplicateMessage::DeleteReplicate(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::UpdateReplicate(gk, lk, sm, e) => {
                ServerReplicateMessage::UpdateReplicate(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerReplicateMessage::AssignPawn(gk, lk) => {
                ServerReplicateMessage::AssignPawn(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::UnassignPawn(gk, lk) => {
                ServerReplicateMessage::UnassignPawn(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::UpdatePawn(gk, lk, sm, e) => {
                ServerReplicateMessage::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerReplicateMessage::CreateEntity(gk, lk, cs) => {
                ServerReplicateMessage::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ServerReplicateMessage::DeleteEntity(gk, lk) => {
                ServerReplicateMessage::DeleteEntity(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::AssignPawnEntity(gk, lk) => {
                ServerReplicateMessage::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::UnassignPawnEntity(gk, lk) => {
                ServerReplicateMessage::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ServerReplicateMessage::AddComponent(lek, gck,lck, r) => {
                ServerReplicateMessage::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
