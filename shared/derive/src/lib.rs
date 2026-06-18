//! # Naia Derive
//! Procedural macros to simplify implementation of Naia types.
//! This crate contains only the naia_shared / crate-internal flavors.
//! Adapter flavors live in their own adapter-owned derive crates.

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

use naia_derive_core::{
    channel::channel_impl,
    message::message_impl,
    replicate::replicate_impl,
};
use quote::quote;
use syn::parse_macro_input;

// Replicate

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate, attributes(replicate))]
pub fn replicate_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_shared };
    replicate_impl(input, shared_crate_name, true).into()
}

// Channel

/// Derives the Channel trait for a given struct
#[proc_macro_derive(Channel)]
pub fn channel_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_shared };
    channel_impl(input, shared_crate_name).into()
}

/// Derives the Channel trait for a given struct, internal to naia-shared
#[proc_macro_derive(ChannelInternal)]
pub fn channel_derive_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { crate };
    channel_impl(input, shared_crate_name).into()
}

// Message

/// Derives the Message trait for a given struct, for internal
#[proc_macro_derive(MessageInternal)]
pub fn message_derive_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, false, false).into()
}

/// Derives the Message trait for a given struct, for FragmentedMessage
#[proc_macro_derive(MessageFragment)]
pub fn message_derive_fragment(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, true, false).into()
}

/// Derives the Message trait for a given struct, for RequestMessage
#[proc_macro_derive(MessageRequest)]
pub fn message_derive_request(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, false, true).into()
}

/// Derives the Message trait for a given struct
#[proc_macro_derive(Message)]
pub fn message_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_shared };
    message_impl(input, shared_crate_name, false, false).into()
}
