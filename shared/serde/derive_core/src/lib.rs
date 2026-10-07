//! # Naia Serde Derive Core
//! Shared implementation logic for naia serde derive crates.
//! All public functions operate on `proc_macro2::TokenStream` and `syn::DeriveInput`
//! so they can be called from any proc-macro crate without a direct proc-macro2
//! crate boundary issue. No adapter or facade crate name appears here.
#![warn(missing_docs)]

/// Per-shape derive implementations (struct, tuple struct, enum).
pub mod impls;

use syn::{Data, DeriveInput, Fields};

/// Dispatches on `input`'s shape to `derive_serde_enum`, `derive_serde_tuple_struct`
/// (unit or tuple struct), or `derive_serde_struct` (named fields), with paths
/// rooted at `serde_crate_name`. Panics (`unimplemented!`) on unions.
pub fn derive_serde_common(
    input: DeriveInput,
    serde_crate_name: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let input_name = input.ident;

    match &input.data {
        Data::Enum(enum_) => {
            impls::derive_serde_enum(enum_, &input_name, &input.generics, serde_crate_name)
        }
        Data::Struct(struct_) => match struct_.fields {
            Fields::Unit | Fields::Unnamed(_) => impls::derive_serde_tuple_struct(
                struct_,
                &input_name,
                &input.generics,
                serde_crate_name,
            ),
            Fields::Named(_) => {
                impls::derive_serde_struct(struct_, &input_name, &input.generics, serde_crate_name)
            }
        },
        _ => unimplemented!("Only structs and enums are supported"),
    }
}
