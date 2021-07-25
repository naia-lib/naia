use naia_shared::{Actor, ActorType, LocalActorKey, Ref, StateMask, LocalEntityKey, EntityKey, LocalComponentKey};

use super::actor_key::{actor_key::ActorKey, ComponentKey};

#[derive(Debug)]
pub enum ServerActorMessage<T: ActorType> {
    CreateActor(ActorKey, LocalActorKey, Ref<dyn Actor<T>>),
    UpdateActor(ActorKey, LocalActorKey, Ref<StateMask>, Ref<dyn Actor<T>>),
    DeleteActor(ActorKey, LocalActorKey),
    AssignPawn(ActorKey, LocalActorKey),
    UnassignPawn(ActorKey, LocalActorKey),
    UpdatePawn(ActorKey, LocalActorKey, Ref<StateMask>, Ref<dyn Actor<T>>),
    CreateEntity(EntityKey, LocalEntityKey),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawnEntity(EntityKey, LocalEntityKey),
    UnassignPawnEntity(EntityKey, LocalEntityKey),
    AddComponent(EntityKey, LocalEntityKey, ComponentKey, LocalComponentKey),
}

impl<T: ActorType> ServerActorMessage<T> {
    pub fn write_message_type(&self) -> u8 {
        match self {
            ServerActorMessage::CreateActor(_, _, _) => 0,
            ServerActorMessage::DeleteActor(_, _) => 1,
            ServerActorMessage::UpdateActor(_, _, _, _) => 2,
            ServerActorMessage::AssignPawn(_, _) => 3,
            ServerActorMessage::UnassignPawn(_, _) => 4,
            ServerActorMessage::UpdatePawn(_, _, _, _) => 5,
            ServerActorMessage::CreateEntity(_, _) => 6,
            ServerActorMessage::DeleteEntity(_, _) => 7,
            ServerActorMessage::AssignPawnEntity(_, _) => 8,
            ServerActorMessage::UnassignPawnEntity(_, _) => 9,
            ServerActorMessage::AddComponent(_, _, _, _) => 10,
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
            ServerActorMessage::CreateEntity(gk, lk) => {
                ServerActorMessage::CreateEntity(gk.clone(), lk.clone())
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
            ServerActorMessage::AddComponent(gek, lek, gck, lck) => {
                ServerActorMessage::AddComponent(gek.clone(), lek.clone(), gck.clone(), lck.clone())
            }
        }
    }
}
