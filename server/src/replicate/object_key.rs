use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod object_key {
    // The Global Key used to get a reference of an Replicate
    new_key_type! { pub struct ObjectKey; }
}

/// Key to be used to reference a Component Replicate
pub type ComponentKey = object_key::ObjectKey;

/// GlobalPawnKey
pub enum GlobalPawnKey {
    /// Replicate
    Replicate(object_key::ObjectKey),
    /// Entity
    Entity(EntityKey),
}