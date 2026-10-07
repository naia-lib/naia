//! # Naia Derive Core
//! Shared implementation logic for naia derive crates.
//! All public functions operate on `proc_macro2::TokenStream` and `syn::DeriveInput`
//! so they can be called from any proc-macro crate without a direct proc-macro2
//! crate boundary issue. No adapter or facade crate name appears here.
#![warn(missing_docs)]
#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

/// `Channel` derive: generates `Channel` and `Named` impls for a unit struct.
pub mod channel;
/// Generates per-marker Bevy client type aliases and an `App` extension trait.
pub mod client_marker;
/// `Message` derive: generates `Message`, `Named`, and `Clone` impls plus a
/// `MessageBuilder` type, for structs and enums.
pub mod message;
/// `Replicate` derive: generates `Replicate`, `Named`, `Clone`, and
/// `HostComponent` impls plus a `ReplicateBuilder` type, for structs.
pub mod replicate;
/// Helpers shared by the derive implementations: struct shape and generics.
pub mod shared;
