use naia_shared::{ActorMutator, Ref};

use super::{actor_key::actor_key::ActorKey, mut_handler::MutHandler};

pub struct ServerActorMutator {
    key: Option<ActorKey>,
    mut_handler: Ref<MutHandler>,
}

impl ServerActorMutator {
    pub fn new(mut_handler: &Ref<MutHandler>) -> Self {
        ServerActorMutator {
            key: None,
            mut_handler: mut_handler.clone(),
        }
    }

    pub fn set_actor_key(&mut self, key: ActorKey) {
        self.key = Some(key);
    }
}

impl ActorMutator for ServerActorMutator {
    fn mutate(&mut self, property_index: u8) {
        if let Some(key) = self.key {
            self.mut_handler.borrow_mut().mutate(&key, property_index);
        }
    }
}
