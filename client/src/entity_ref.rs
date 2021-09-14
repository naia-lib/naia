use naia_shared::{LocalEntityKey, ProtocolType, Ref, Replicate};

use super::client::Client;

// EntityRef
#[derive(Debug)]
pub struct EntityRef<'s, P: ProtocolType> {
    client: &'s Client<P>,
    key: LocalEntityKey,
}

impl<'s, P: ProtocolType> EntityRef<'s, P> {
    pub fn new(client: &'s Client<P>, key: &LocalEntityKey) -> Self {
        EntityRef { client, key: *key }
    }

    pub fn key(&self) -> LocalEntityKey {
        self.key
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.client.entity_contains_type::<R>(&self.key);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.client.component::<R>(&self.key);
    }

    pub fn is_owned(&self) -> bool {
        return self.client.entity_is_owned(&self.key);
    }

    pub fn prediction(&self) -> PredictedEntityRef<P> {
        if !self.is_owned() {
            panic!("Attempted to call .prediction() on an un-owned Entity!");
        }
        return PredictedEntityRef::new(self.client, &self.key);
    }
}

// PredictedEntityRef
#[derive(Debug)]
pub struct PredictedEntityRef<'s, P: ProtocolType> {
    client: &'s Client<P>,
    key: LocalEntityKey,
}

impl<'s, P: ProtocolType> PredictedEntityRef<'s, P> {
    pub fn new(client: &'s Client<P>, key: &LocalEntityKey) -> Self {
        PredictedEntityRef { client, key: *key }
    }

    pub fn key(&self) -> LocalEntityKey {
        self.key
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.client.entity_contains_type::<R>(&self.key);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.client.component_prediction::<R>(&self.key);
    }
}
