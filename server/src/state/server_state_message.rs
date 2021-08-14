use naia_shared::{State, ProtocolType, LocalObjectKey, Ref, DiffMask, LocalEntityKey, EntityKey, LocalComponentKey, StateMessageType};

use super::object_key::{object_key::ObjectKey, ComponentKey};

#[derive(Debug)]
pub enum ServerStateMessage<T: ProtocolType> {
    CreateState(ObjectKey, LocalObjectKey, Ref<dyn State<T>>),
    UpdateState(ObjectKey, LocalObjectKey, Ref<DiffMask>, Ref<dyn State<T>>),
    DeleteState(ObjectKey, LocalObjectKey),
    AssignPawn(ObjectKey, LocalObjectKey),
    UnassignPawn(ObjectKey, LocalObjectKey),
    UpdatePawn(ObjectKey, LocalObjectKey, Ref<DiffMask>, Ref<dyn State<T>>),
    CreateEntity(EntityKey, LocalEntityKey, Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn State<T>>)>>),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(LocalEntityKey, ComponentKey, LocalComponentKey, Ref<dyn State<T>>),
}

impl<T: ProtocolType> ServerStateMessage<T> {
    pub fn as_type(&self) -> StateMessageType {
        match self {
            ServerStateMessage::CreateState(_, _, _) => StateMessageType::CreateState,
            ServerStateMessage::DeleteState(_, _) => StateMessageType::DeleteState,
            ServerStateMessage::UpdateState(_, _, _, _) => StateMessageType::UpdateState,
            ServerStateMessage::AssignPawn(_, _) => StateMessageType::AssignPawn,
            ServerStateMessage::UnassignPawn(_, _) => StateMessageType::UnassignPawn,
            ServerStateMessage::UpdatePawn(_, _, _, _) => StateMessageType::UpdatePawn,
            ServerStateMessage::CreateEntity(_, _, _) => StateMessageType::CreateEntity,
            ServerStateMessage::DeleteEntity(_, _) => StateMessageType::DeleteEntity,
            ServerStateMessage::AssignPawnEntity(_, _) => StateMessageType::AssignPawnEntity,
            ServerStateMessage::UnassignPawnEntity(_, _) => StateMessageType::UnassignPawnEntity,
            ServerStateMessage::AddComponent(_, _, _, _) => StateMessageType::AddComponent,
        }
    }
}

impl<T: ProtocolType> Clone for ServerStateMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerStateMessage::CreateState(gk, lk, e) => {
                ServerStateMessage::CreateState(gk.clone(), lk.clone(), e.clone())
            }
            ServerStateMessage::DeleteState(gk, lk) => {
                ServerStateMessage::DeleteState(gk.clone(), lk.clone())
            }
            ServerStateMessage::UpdateState(gk, lk, sm, e) => {
                ServerStateMessage::UpdateState(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerStateMessage::AssignPawn(gk, lk) => {
                ServerStateMessage::AssignPawn(gk.clone(), lk.clone())
            }
            ServerStateMessage::UnassignPawn(gk, lk) => {
                ServerStateMessage::UnassignPawn(gk.clone(), lk.clone())
            }
            ServerStateMessage::UpdatePawn(gk, lk, sm, e) => {
                ServerStateMessage::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerStateMessage::CreateEntity(gk, lk, cs) => {
                ServerStateMessage::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ServerStateMessage::DeleteEntity(gk, lk) => {
                ServerStateMessage::DeleteEntity(gk.clone(), lk.clone())
            }
            ServerStateMessage::AssignPawnEntity(gk, lk) => {
                ServerStateMessage::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ServerStateMessage::UnassignPawnEntity(gk, lk) => {
                ServerStateMessage::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ServerStateMessage::AddComponent(lek, gck,lck, r) => {
                ServerStateMessage::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
