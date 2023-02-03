//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Replicate

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

mod replicate;

use replicate::replicate_impl;

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate, attributes(protocol_path))]
pub fn replicate_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    replicate_impl(input)
}