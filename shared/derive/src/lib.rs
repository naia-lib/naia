//! # Naia Derive
//! Procedural macros to simplify implementation of Naia types

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

use quote::quote;

mod channel;
mod message;
mod replicate;
mod shared;

use channel::channel_impl;
use message::message_impl;
use replicate::replicate_impl;

// Replicate

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate)]
pub fn replicate_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_shared };
    replicate_impl(input, shared_crate_name)
}

/// Derives the Replicate trait for a given struct, for the Bevy adapter
#[proc_macro_derive(ReplicateBevy)]
pub fn replicate_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_bevy_shared };
    replicate_impl(input, shared_crate_name)
}

/// Derives the Replicate trait for a given struct, for the Bevy adapter
#[proc_macro_derive(ReplicateHecs)]
pub fn replicate_derive_hecs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_hecs_shared };
    replicate_impl(input, shared_crate_name)
}

// Channel

/// Derives the Channel trait for a given struct
#[proc_macro_derive(Channel)]
pub fn channel_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    channel_impl(input)
}

// Message

/// Derives the Message trait for a given struct, for internal
#[proc_macro_derive(MessageInternal)]
pub fn message_derive_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, false)
}

/// Derives the Message trait for a given struct, for FragmentedMessage
#[proc_macro_derive(MessageFragment)]
pub fn message_derive_fragment(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, true)
}

/// Derives the Message trait for a given struct
#[proc_macro_derive(Message)]
pub fn message_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_shared };
    message_impl(input, shared_crate_name, false)
}

/// Derives the Message trait for a given struct, for the Bevy adapter
#[proc_macro_derive(MessageBevy)]
pub fn message_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_bevy_shared };
    message_impl(input, shared_crate_name, false)
}

/// Derives the Message trait for a given struct, for the Hecs adapter
#[proc_macro_derive(MessageHecs)]
pub fn message_derive_hecs(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_hecs_shared };
    message_impl(input, shared_crate_name, false)
}
