//! Sealed trait alias for Replicated Resource types.
//!
//! A `ReplicatedResource` is a type that can be both a Bevy `Resource`
//! (for user-facing `Res<R>` / `ResMut<R>` access) AND a naia
//! `Replicate` (for wire-format diff-tracked replication) AND a
//! mutable Bevy `Component` (for the hidden 1-component entity that
//! carries the wire state).
//!
//! Users derive `Replicate` (which auto-derives `Component`) and add
//! `#[derive(Resource)]`. The blanket `impl ReplicatedResource for T`
//! below picks them up automatically. Internal API surfaces use
//! `R: ReplicatedResource` instead of repeating the three bounds.

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::resource::Resource;

use crate::Replicate;

/// Type bound for a Replicated Resource. A type satisfying this bound
/// is:
///
/// - A `naia::Replicate` (registered via `protocol.add_resource::<R>()`).
/// - A Bevy `Resource` (for `Res<R>`/`ResMut<R>` access).
/// - A mutable Bevy `Component` (for the hidden resource entity).
///
/// A blanket impl covers any `T` satisfying all three; users do not
/// implement this trait directly.
pub trait ReplicatedResource: Replicate + Resource + Component<Mutability = Mutable> {}

impl<T> ReplicatedResource for T where
    T: Replicate + Resource + Component<Mutability = Mutable>
{
}
