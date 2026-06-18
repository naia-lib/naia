//! # Naia Bevy Derive
//! Proc-macro derives for the naia Bevy adapter.
//! Hosts the Bevy-flavored Replicate, Message, and ClientMarker derives.

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

use naia_derive_core::{
    channel::channel_impl,
    client_marker::client_marker_impl,
    message::message_impl,
    replicate::replicate_impl,
};
use quote::quote;
use syn::parse_macro_input;

/// Derives the Replicate trait for a given struct, for the Bevy adapter.
/// Users add their own `#[derive(Component)]` alongside this derive, so we
/// skip auto-emitting a Bevy Component impl here.
#[proc_macro_derive(Replicate, attributes(replicate))]
pub fn replicate_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_bevy_shared };
    replicate_impl(input, shared_crate_name, false).into()
}

/// Derives the Message trait for a given struct, for the Bevy adapter
#[proc_macro_derive(Message)]
pub fn message_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_bevy_shared };
    message_impl(input, shared_crate_name, false, false).into()
}

/// Derives the Channel trait for a given struct, for the Bevy adapter.
#[proc_macro_derive(Channel)]
pub fn channel_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let shared_crate_name = quote! { naia_bevy_shared };
    channel_impl(input, shared_crate_name).into()
}

/// Derives the per-marker naia client facet for the Bevy adapter.
/// Emits type aliases and an `{Marker}AppBundleExt` trait using `naia_bevy_client::` paths.
#[proc_macro_derive(ClientMarker)]
pub fn client_marker_derive_bevy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let root = quote! { naia_bevy_client };
    client_marker_impl(input, root).into()
}
