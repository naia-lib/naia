use naia_shared::{Actor, ActorType, LocalActorKey, Ref, StateMask, LocalEntityKey, EntityKey, LocalComponentKey, ActorMessageType};

use super::actor_key::{actor_key::ActorKey, ComponentKey};

#[derive(Debug)]
pub enum ServerActorMessage<T: ActorType> {
    CreateActor(ActorKey, LocalActorKey, Ref<dyn Actor<T>>),
    UpdateActor(ActorKey, LocalActorKey, Ref<StateMask>, Ref<dyn Actor<T>>),
    DeleteActor(ActorKey, LocalActorKey),
    AssignPawn(ActorKey, LocalActorKey),
    UnassignPawn(ActorKey, LocalActorKey),
    UpdatePawn(ActorKey, LocalActorKey, Ref<StateMask>, Ref<dyn Actor<T>>),
    CreateEntity(EntityKey, LocalEntityKey, Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn Actor<T>>)>>),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(LocalEntityKey, ComponentKey, LocalComponentKey, Ref<dyn Actor<T>>),
}

impl<T: ActorType> ServerActorMessage<T> {
    pub fn as_type(&self) -> ActorMessageType {
        match self {
            ServerActorMessage::CreateActor(_, _, _) => ActorMessageType::CreateActor,
            ServerActorMessage::DeleteActor(_, _) => ActorMessageType::DeleteActor,
            ServerActorMessage::UpdateActor(_, _, _, _) => ActorMessageType::UpdateActor,
            ServerActorMessage::AssignPawn(_, _) => ActorMessageType::AssignPawn,
            ServerActorMessage::UnassignPawn(_, _) => ActorMessageType::UnassignPawn,
            ServerActorMessage::UpdatePawn(_, _, _, _) => ActorMessageType::UpdatePawn,
            ServerActorMessage::CreateEntity(_, _, _) => ActorMessageType::CreateEntity,
            ServerActorMessage::DeleteEntity(_, _) => ActorMessageType::DeleteEntity,
            ServerActorMessage::AssignPawnEntity(_, _) => ActorMessageType::AssignPawnEntity,
            ServerActorMessage::UnassignPawnEntity(_, _) => ActorMessageType::UnassignPawnEntity,
            ServerActorMessage::AddComponent(_, _, _, _) => ActorMessageType::AddComponent,
        }
    }
}

impl<T: ActorType> Clone for ServerActorMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerActorMessage::CreateActor(gk, lk, e) => {
                ServerActorMessage::CreateActor(gk.clone(), lk.clone(), e.clone())
            }
            ServerActorMessage::DeleteActor(gk, lk) => {
                ServerActorMessage::DeleteActor(gk.clone(), lk.clone())
            }
            ServerActorMessage::UpdateActor(gk, lk, sm, e) => {
                ServerActorMessage::UpdateActor(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerActorMessage::AssignPawn(gk, lk) => {
                ServerActorMessage::AssignPawn(gk.clone(), lk.clone())
            }
            ServerActorMessage::UnassignPawn(gk, lk) => {
                ServerActorMessage::UnassignPawn(gk.clone(), lk.clone())
            }
            ServerActorMessage::UpdatePawn(gk, lk, sm, e) => {
                ServerActorMessage::UpdatePawn(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerActorMessage::CreateEntity(gk, lk, cs) => {
                ServerActorMessage::CreateEntity(gk.clone(), lk.clone(), cs.clone())
            }
            ServerActorMessage::DeleteEntity(gk, lk) => {
                ServerActorMessage::DeleteEntity(gk.clone(), lk.clone())
            }
            ServerActorMessage::AssignPawnEntity(gk, lk) => {
                ServerActorMessage::AssignPawnEntity(gk.clone(), lk.clone())
            }
            ServerActorMessage::UnassignPawnEntity(gk, lk) => {
                ServerActorMessage::UnassignPawnEntity(gk.clone(), lk.clone())
            }
            ServerActorMessage::AddComponent(lek, gck,lck, r) => {
                ServerActorMessage::AddComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
        }
    }
}
