use std::ops::Deref;

/// A EntityType aggregates all traits needed to be used as an Entity
pub trait EntityType: Copy + Clone + PartialEq + Eq + Deref + std::hash::Hash + 'static {}