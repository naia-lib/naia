use std::collections::VecDeque;

use naia_shared::{LocalEntityKey, ActorType};

use super::{locality_status::LocalityStatus, server_actor_message::ServerActorMessage};

#[derive(Debug)]
pub struct EntityRecord<T: ActorType> {
    pub local_key: LocalEntityKey,
    pub status: LocalityStatus,
    pub on_create_messages: VecDeque<ServerActorMessage<T>>,
}

impl<T: ActorType> EntityRecord<T> {
    pub fn new(local_key: LocalEntityKey) -> Self {
        EntityRecord {
            local_key,
            status: LocalityStatus::Creating,
            on_create_messages: VecDeque::new(),
        }
    }
}
