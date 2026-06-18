//! # Naia Derive Core
//! Shared implementation logic for naia derive crates.
//! All public functions operate on `proc_macro2::TokenStream` and `syn::DeriveInput`
//! so they can be called from any proc-macro crate without a direct proc-macro2
//! crate boundary issue. No adapter or facade crate name appears here.

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

pub mod channel;
pub mod client_marker;
pub mod message;
pub mod replicate;
pub mod shared;
