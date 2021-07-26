use naia_shared::{LocalActorKey, LocalEntityKey, EventType};

use super::ClientEvent;

#[derive(Debug, Clone)]
pub enum ClientActorMessage {
    CreateActor(LocalActorKey),
    UpdateActor(LocalActorKey),
    DeleteActor(LocalActorKey),
    AssignPawn(LocalActorKey),
    UnassignPawn(LocalActorKey),
    ResetPawn(LocalActorKey),
    CreateEntity(LocalEntityKey),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, LocalActorKey),
    UpdateComponent(LocalEntityKey, LocalActorKey),
    RemoveComponent(LocalEntityKey, LocalActorKey),
}

impl ClientActorMessage {
    pub fn to_event<T: EventType>(&self) -> ClientEvent<T> {
        match self {
            ClientActorMessage::CreateActor(local_key) => {
                return ClientEvent::CreateActor(*local_key);
            }
            ClientActorMessage::DeleteActor(local_key) => {
                return ClientEvent::DeleteActor(*local_key);
            }
            ClientActorMessage::UpdateActor(local_key) => {
                return ClientEvent::UpdateActor(*local_key);
            }
            ClientActorMessage::AssignPawn(local_key) => {
                return ClientEvent::AssignPawn(*local_key);
            }
            ClientActorMessage::UnassignPawn(local_key) => {
                return ClientEvent::UnassignPawn(*local_key);
            }
            ClientActorMessage::ResetPawn(local_key) => {
                return ClientEvent::ResetPawn(*local_key);
            }
            ClientActorMessage::CreateEntity(local_key) => {
                return ClientEvent::CreateEntity(*local_key);
            }
            ClientActorMessage::DeleteEntity(local_key) => {
                return ClientEvent::DeleteEntity(*local_key);
            }
            ClientActorMessage::AssignPawnEntity(local_key) => {
                return ClientEvent::AssignPawnEntity(*local_key);
            }
            ClientActorMessage::UnassignPawnEntity(local_key) => {
                return ClientEvent::UnassignPawnEntity(*local_key);
            }
            ClientActorMessage::ResetPawnEntity(local_key) => {
                return ClientEvent::ResetPawnEntity(*local_key);
            }
            ClientActorMessage::AddComponent(entity_key, component_key) => {
                return ClientEvent::AddComponent(*entity_key, *component_key);
            }
            ClientActorMessage::UpdateComponent(entity_key, component_key) => {
                return ClientEvent::UpdateComponent(*entity_key, *component_key);
            }
            ClientActorMessage::RemoveComponent(entity_key, component_key) => {
                return ClientEvent::RemoveComponent(*entity_key, *component_key);
            }
        }
    }
}
