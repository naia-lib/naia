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

use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

mod impls;
use impls::*;

#[proc_macro_derive(Serde)]
pub fn derive_serde(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let serde_crate_name = quote! { naia_shared };
    derive_serde_common(input, serde_crate_name)
}

#[proc_macro_derive(SerdeInternal)]
pub fn derive_serde_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let serde_crate_name = quote! { naia_serde };
    derive_serde_common(input, serde_crate_name)
}

#[proc_macro_derive(SerdeBevyShared)]
pub fn derive_serde_bevy_shared(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let serde_crate_name = quote! { naia_bevy_shared };
    derive_serde_common(input, serde_crate_name)
}

#[proc_macro_derive(SerdeBevyServer)]
pub fn derive_serde_bevy_server(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let serde_crate_name = quote! { naia_bevy_server };
    derive_serde_common(input, serde_crate_name)
}

#[proc_macro_derive(SerdeBevyClient)]
pub fn derive_serde_bevy_client(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let serde_crate_name = quote! { naia_bevy_client };
    derive_serde_common(input, serde_crate_name)
}

fn derive_serde_common(
    input: proc_macro::TokenStream,
    serde_crate_name: proc_macro2::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let input_name = input.ident;

    let gen = match &input.data {
        Data::Enum(enum_) => derive_serde_enum(enum_, &input_name, serde_crate_name),
        Data::Struct(struct_) => match struct_.fields {
            Fields::Unit | Fields::Unnamed(_) => {
                derive_serde_tuple_struct(struct_, &input_name, serde_crate_name)
            }
            Fields::Named(_) => derive_serde_struct(struct_, &input_name, serde_crate_name),
        },
        _ => unimplemented!("Only structs and enums are supported"),
    };

    proc_macro::TokenStream::from(gen)
}
