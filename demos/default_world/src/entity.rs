use std::ops::Deref;

use naia_shared::EntityType;

// Entity

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
mod entity {
    // The Key used to reference an Entity
    new_key_type! { pub struct Entity; }
}

use entity::Entity as Key;

pub type Entity = Key;

impl Deref for Entity {
    type Target = Self;

    fn deref(&self) -> &Self {
        &self
    }
}

impl EntityType for Entity {}
