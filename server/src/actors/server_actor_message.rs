use naia_shared::{Actor, ActorType, LocalActorKey, Ref, StateMask};

use super::actor_key::actor_key::ActorKey;

#[derive(Debug)]
pub enum ServerActorMessage<T: ActorType> {
    CreateActor(ActorKey, LocalActorKey, Ref<dyn Actor<T>>),
    UpdateActor(ActorKey, LocalActorKey, Ref<StateMask>, Ref<dyn Actor<T>>),
    DeleteActor(ActorKey, LocalActorKey),
    AssignPawn(ActorKey, LocalActorKey),
    UnassignPawn(ActorKey, LocalActorKey),
}

impl<T: ActorType> ServerActorMessage<T> {
    pub fn write_message_type(&self) -> u8 {
        match self {
            ServerActorMessage::CreateActor(_, _, _) => 0,
            ServerActorMessage::DeleteActor(_, _) => 1,
            ServerActorMessage::UpdateActor(_, _, _, _) => 2,
            ServerActorMessage::AssignPawn(_, _) => 3,
            ServerActorMessage::UnassignPawn(_, _) => 4,
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
        }
    }
}
