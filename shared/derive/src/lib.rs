//! # Naia Derive
//! Procedural macros to simplify implementation of Naia types

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

use quote::quote;

mod channel;
mod client_marker;
mod message;
mod replicate;
mod shared;

use channel::channel_impl;
use client_marker::client_marker_impl;
use message::message_impl;
use replicate::replicate_impl;

// Replicate

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate, attributes(replicate))]
pub fn replicate_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_shared };
    replicate_impl(input, shared_crate_name, true)
}

/// Derives the Replicate trait for a given struct, for the Bevy adapter.
/// Users add their own `#[derive(Component)]` alongside this derive, so we
/// skip auto-emitting a Bevy Component impl here.
#[proc_macro_derive(ReplicateBevy, attributes(replicate))]
pub fn replicate_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_bevy_shared };
    replicate_impl(input, shared_crate_name, false)
}

/// Derives the Replicate trait for a given struct, for the Diax facade.
/// Mirror of ReplicateBevy: emits `diax_net_shared::` paths and skips the
/// auto Component impl (the proto crate adds `#[derive(Component)]` itself).
#[proc_macro_derive(ReplicateDiax, attributes(replicate))]
pub fn replicate_derive_diax(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { diax_net_shared };
    replicate_impl(input, shared_crate_name, false)
}

// ClientMarker

/// Derives the per-marker naia client facet for the Bevy adapter.
/// Emits type aliases and an `{Marker}AppBundleExt` trait using `naia_bevy_client::` paths.
#[proc_macro_derive(ClientMarkerBevy)]
pub fn client_marker_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let root = quote! { naia_bevy_client };
    client_marker_impl(input, root)
}

/// Derives the per-marker naia client facet for the Diax facade.
/// Emits type aliases and an `{Marker}AppBundleExt` trait using `diax_net_client::` paths.
#[proc_macro_derive(ClientMarkerDiax)]
pub fn client_marker_derive_diax(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let root = quote! { diax_net_client };
    client_marker_impl(input, root)
}

// Channel

/// Derives the Channel trait for a given struct
#[proc_macro_derive(Channel)]
pub fn channel_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_shared };
    channel_impl(input, shared_crate_name)
}

/// Derives the Channel trait for a given struct, for the Diax facade.
/// Emits `diax_net_shared::` paths (Channel has no Bevy flavor; this is the
/// only thing that lets the proto crates shed their direct `naia-shared` dep).
#[proc_macro_derive(ChannelDiax)]
pub fn channel_derive_diax(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { diax_net_shared };
    channel_impl(input, shared_crate_name)
}

/// Derives the Channel trait for a given struct, internal to naia-shared
#[proc_macro_derive(ChannelInternal)]
pub fn channel_derive_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    channel_impl(input, shared_crate_name)
}

// Message

/// Derives the Message trait for a given struct, for internal
#[proc_macro_derive(MessageInternal)]
pub fn message_derive_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, false, false)
}

/// Derives the Message trait for a given struct, for FragmentedMessage
#[proc_macro_derive(MessageFragment)]
pub fn message_derive_fragment(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, true, false)
}

/// Derives the Message trait for a given struct, for RequestMessage
#[proc_macro_derive(MessageRequest)]
pub fn message_derive_request(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { crate };
    message_impl(input, shared_crate_name, false, true)
}

/// Derives the Message trait for a given struct
#[proc_macro_derive(Message)]
pub fn message_derive_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_shared };
    message_impl(input, shared_crate_name, false, false)
}

/// Derives the Message trait for a given struct, for the Bevy adapter
#[proc_macro_derive(MessageBevy)]
pub fn message_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { naia_bevy_shared };
    message_impl(input, shared_crate_name, false, false)
}

/// Derives the Message trait for a given struct, for the Diax facade.
/// Mirror of MessageBevy: emits `diax_net_shared::` paths.
#[proc_macro_derive(MessageDiax)]
pub fn message_derive_diax(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let shared_crate_name = quote! { diax_net_shared };
    message_impl(input, shared_crate_name, false, false)
}
