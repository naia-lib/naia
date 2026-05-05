//! Shared world-scan helpers for Replicated Resource lookup in tests.
//!
//! V1 the test harness lookups by scanning entities for one carrying
//! the resource component (the proper client-side `ResourceRegistry`
//! in the bevy adapter is the production path; the harness uses
//! `naia_demo_world` directly so a scan is fine here). Lifted into a
//! shared helper so the four ctx types (`Server{Mutate,Expect}Ctx`,
//! `Client{Mutate,Expect}Ctx`) don't each carry a duplicated copy.

use naia_shared::{ReplicatedComponent, WorldRefType};

/// True iff `world` contains an entity carrying `R`.
pub(crate) fn has_resource_in_world<R, W>(world: &W) -> bool
where
    R: ReplicatedComponent,
    W: WorldRefType<crate::TestEntity>,
{
    for e in world.entities() {
        if world.has_component::<R>(&e) {
            return true;
        }
    }
    false
}

/// Find the world entity carrying `R`, if any.
pub(crate) fn find_resource_entity_in_world<R, W>(world: &W) -> Option<crate::TestEntity>
where
    R: ReplicatedComponent,
    W: WorldRefType<crate::TestEntity>,
{
    for e in world.entities() {
        if world.has_component::<R>(&e) {
            return Some(e);
        }
    }
    None
}

/// Read-only access to the value of a resource of type `R` in `world`.
/// Closure receives `&R`.
pub(crate) fn read_resource_in_world<R, W, F, T>(world: &W, f: F) -> Option<T>
where
    R: ReplicatedComponent,
    W: WorldRefType<crate::TestEntity>,
    F: FnOnce(&R) -> T,
{
    for e in world.entities() {
        if let Some(comp) = world.component::<R>(&e) {
            return Some(f(&*comp));
        }
    }
    None
}
