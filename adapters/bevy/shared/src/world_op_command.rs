//! Generic `WorldOpCommand<F>` — collapses the boilerplate of "queue a
//! Bevy `Command` that runs with `&mut World`" used by every naia bevy
//! adapter Commands extension.
//!
//! Before this helper, every world-mutating Commands extension method
//! defined its own one-shot `Command` struct (e.g.
//! `ConfigureReplicationCommand`, `ReplicateResourceCommand`,
//! `RemoveReplicatedResourceCommand`, etc.) with the same shape:
//!
//! ```ignore
//! struct FooCommand { ... }
//! impl Command for FooCommand {
//!     fn apply(self, world: &mut World) {
//!         world.resource_scope(|world, mut server| { ... });
//!     }
//! }
//! ```
//!
//! That's a lot of repeated structure for what is, in essence, "queue
//! this closure to run with `&mut World`." With `WorldOpCommand`:
//!
//! ```ignore
//! commands.queue(WorldOpCommand::new(move |world| {
//!     world.resource_scope::<ServerImpl, _>(|world, mut server| { ... });
//! }));
//! ```
//!
//! Trade-off: the generic over `F` means each call site instantiates
//! its own concrete type. Small compile-time cost, big readability win
//! (no more per-operation Command struct + Command impl).

use bevy_ecs::system::Command;
use bevy_ecs::world::World;

/// A Bevy `Command` whose `apply` body is an arbitrary `FnOnce(&mut World)`.
///
/// Use this when you need to run code with `&mut World` deferred to the
/// next `apply_deferred` boundary. Saves the boilerplate of defining a
/// dedicated `Command` struct + impl per operation.
pub struct WorldOpCommand<F: FnOnce(&mut World) + Send + 'static> {
    op: F,
}

impl<F: FnOnce(&mut World) + Send + 'static> WorldOpCommand<F> {
    /// Wrap a closure as a Bevy `Command`. Queue with
    /// `commands.queue(WorldOpCommand::new(|world| ...))`.
    #[inline]
    pub fn new(op: F) -> Self {
        Self { op }
    }
}

impl<F: FnOnce(&mut World) + Send + 'static> Command for WorldOpCommand<F> {
    #[inline]
    fn apply(self, world: &mut World) {
        (self.op)(world);
    }
}
