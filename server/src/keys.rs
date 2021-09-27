#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod entity_key {
    // The Global Key used to get a reference of a Entity
    new_key_type! { pub struct EntityKey; }
}

pub mod component_key {
    // The Global Key used to get a reference of a Component
    new_key_type! { pub struct ComponentKey; }
}
