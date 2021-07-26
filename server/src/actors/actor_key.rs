use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod actor_key {
    // The Global Key used to get a reference of an Actor
    new_key_type! { pub struct ActorKey; }
}

/// Key to be used to reference a Component Actor
pub type ComponentKey = actor_key::ActorKey;

/// GlobalPawnKey
pub enum GlobalPawnKey {
    /// Actor
    Actor(actor_key::ActorKey),
    /// Entity
    Entity(EntityKey),
}