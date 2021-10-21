#[allow(missing_docs)]
#[allow(unused_doc_comments)]
mod global_component_key {
    // The Key used to get a reference of a Component
    new_key_type! { pub struct ComponentKey; }
}

pub use global_component_key::ComponentKey;
