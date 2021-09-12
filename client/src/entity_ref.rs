use naia_shared::{ProtocolType, Ref, Replicate};

use crate::{Client, LocalEntityKey};

// PastEntityRef
#[derive(Debug)]
pub struct PastEntityRef<'s, P: ProtocolType> {
    client: &'s Client<P>,
    key: LocalEntityKey,
}

impl<'s, P: ProtocolType> PastEntityRef<'s, P> {
    pub fn new(client: &'s Client<P>, key: &LocalEntityKey) -> Self {
        PastEntityRef { client, key: *key }
    }

    pub fn key(&self) -> LocalEntityKey {
        self.key
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.client.entity_contains_type::<R>(&self.key);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.client.component_past::<R>(&self.key);
    }
}

// PresentEntityRef
#[derive(Debug)]
pub struct PresentEntityRef<'s, P: ProtocolType> {
    client: &'s Client<P>,
    key: LocalEntityKey,
}

impl<'s, P: ProtocolType> PresentEntityRef<'s, P> {
    pub fn new(client: &'s Client<P>, key: &LocalEntityKey) -> Self {
        PresentEntityRef { client, key: *key }
    }

    pub fn key(&self) -> LocalEntityKey {
        self.key
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.client.entity_contains_type::<R>(&self.key);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.client.component_present::<R>(&self.key);
    }
}
