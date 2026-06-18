//! `naia-serde-derive` — derives `Serde` implementations for structs, tuple
//! structs, and enums.
//!
//! ## Bit-budget invariant (load-bearing)
//!
//! For every `T: Serde`, `T.bit_length()` returns exactly the number of bits
//! `T.ser()` writes against any `BitWrite`. The derive emits both methods
//! from the same field/variant traversal so they can never drift, and the
//! `naia_serde::tests::bit_budget` proptest harness pins this contract for
//! every derive shape (named struct, tuple struct, unit/payload enum) and
//! for the `SerdeInteger` family.
//!
//! Phase 9.2 history: a 5-year-old off-by-one in derived-enum
//! `bits_needed_for(N)` shipped through every prior phase before being
//! caught in Phase 8.3. The proptest harness exists to keep that class of
//! bug from reappearing.
//!
//! This crate contains only the naia_shared / crate-internal (naia_serde) flavors.
//! Adapter and facade flavors live in their own adapter-owned derive crates.

use naia_serde_derive_core::derive_serde_common;
use quote::quote;
use syn::parse_macro_input;

#[proc_macro_derive(Serde)]
pub fn derive_serde(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let serde_crate_name = quote! { naia_shared };
    derive_serde_common(input, serde_crate_name).into()
}

#[proc_macro_derive(SerdeInternal)]
pub fn derive_serde_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let serde_crate_name = quote! { naia_serde };
    derive_serde_common(input, serde_crate_name).into()
}
