//! Bevy-resource ↔ entity-component mirror for Replicated Resources
//! (Mode B of `_AGENTS/RESOURCES_PLAN.md` §4.5).
//!
//! ## Status: deferred (Mode A is in)
//!
//! V1 ships with **Mode A**: users access replicated resources via
//! `Query<&R>` / `Query<&mut R>` over the hidden resource entity. This
//! works because Naia's `#[derive(Replicate)]` already emits
//! `impl Component for R` (under `bevy_support`), so `R` is a normal
//! Bevy `Component` on a hidden 1-component entity. Mutations via
//! `Query<&mut R>` fire the entity-component's `PropertyMutator` and
//! replicate normally.
//!
//! **Mode B** — surfacing as standard `Res<R>` / `ResMut<R>` — requires
//! either:
//!
//! 1. Adding `mirror_field(idx, other: &dyn Replicate)` to the
//!    `Replicate` trait + derive macro so the per-tick sync system can
//!    propagate only the specific Property fields the user touched in
//!    `ResMut<R>`, OR
//! 2. Calling `Replicate::mirror` unconditionally on `Changed<R>`,
//!    which over-replicates within the changed window (every field
//!    sent on each `ResMut<R>` access, regardless of which one was
//!    actually mutated).
//!
//! Option 1 is the correct long-term answer (matches per-field diff
//! semantics that Components already enjoy) but requires touching the
//! shared derive macro. Option 2 is a one-tick over-replication tax
//! that's acceptable for many resources but would regress relative to
//! how Components replicate today.
//!
//! Tracking decision: ship Mode A in V1 (no over-replication, no
//! derive-macro change), file Mode B as a follow-up issue with the
//! `mirror_field` trait extension on the design table. The user-facing
//! API in `commands.rs` is forward-compatible — `replicate_resource(value)`
//! does not change shape between Mode A and Mode B.
//!
//! See `_AGENTS/RESOURCES_PLAN.md` §4.5 for the full mirror design.
