pub mod local_entity;
pub mod local_entity_map;
mod local_entity_record;
pub mod local_world_manager;

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        mod interior_visibility;
        pub use interior_visibility::LocalEntity;
    }
}
