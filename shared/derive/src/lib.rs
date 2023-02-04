//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Replicate

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

mod replicate;
mod channel;
mod message;
mod shared;

use replicate::replicate_impl;
use channel::channel_impl;
use message::message_impl;

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate)]
pub fn replicate_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    replicate_impl(input)
}

/// Derives the Channel trait for a given struct
#[proc_macro_derive(Channel)]
pub fn channel_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    channel_impl(input)
}

/// Derives the Message trait for a given struct
#[proc_macro_derive(Message)]
pub fn message_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    message_impl(input)
}
