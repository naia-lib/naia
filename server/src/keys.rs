use std::any::TypeId;

/// A KeyType aggregates all traits needed to be implemented to be used as an Entity Key
pub trait KeyType: Copy + Clone + PartialEq + Eq + std::hash::Hash + 'static {}

/// A ComponentKey includes information necessary to look up a Component for a specific Entity
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentKey<K: KeyType> {
    entity_key: K,
    component_type: TypeId,
}

impl<K: KeyType> ComponentKey<K> {
    /// Create a new ComponentKey
    pub fn new(entity_key: &K, component_type: &TypeId) -> Self {
        ComponentKey {
            entity_key: *entity_key,
            component_type: *component_type
        }
    }

    /// Get the ComponentKey's underlying Entity Key
    pub fn entity_key(&self) -> &K {
        &self.entity_key
    }

    /// Get the ComponentKey's underlying Component TypeId
    pub fn component_type(&self) -> &TypeId {
        &self.component_type
    }
}