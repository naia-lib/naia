use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod replica_key {
    // The Global Key used to get a reference of an Replica
    new_key_type! { pub struct ReplicaKey; }
}

/// Key to be used to reference an Object Replica
pub type ObjectKey = replica_key::ReplicaKey;

/// Key to be used to reference a Component Replica
pub type ComponentKey = replica_key::ReplicaKey;

/// GlobalPawnKey
pub enum GlobalPawnKey {
    /// Object
    Object(ObjectKey),
    /// Entity
    Entity(EntityKey),
}
