use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod object_key {
    // The Global Key used to get a reference of an State
    new_key_type! { pub struct ObjectKey; }
}

/// Key to be used to reference a Component State
pub type ComponentKey = object_key::ObjectKey;

/// GlobalPawnKey
pub enum GlobalPawnKey {
    /// State
    State(object_key::ObjectKey),
    /// Entity
    Entity(EntityKey),
}