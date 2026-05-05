//! Replicated Resource primitive.
//!
//! Naia's analog to Bevy's `Resource`: a per-`World` singleton with
//! diff-tracked, per-field replication. Implemented internally as a hidden
//! 1-component entity, reusing the entity replication pipeline. The two
//! types in this module are the only registry-layer additions; everything
//! else is reused from the entity machinery.
//!
//! - `ResourceKinds`: protocol-wide marker recording which `ComponentKind`s
//!   are resources. Receiver-side, this is checked on `SpawnWithComponents`
//!   to populate `ResourceRegistry`.
//! - `ResourceRegistry`: per-`World` `TypeId<R> ↔ GlobalEntity` map,
//!   maintained on both sender and receiver sides.

pub mod resource_kinds;
pub mod resource_registry;

pub use resource_kinds::ResourceKinds;
pub use resource_registry::ResourceRegistry;
