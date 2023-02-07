use syn::{parse_macro_input, Data, DeriveInput, Fields};

mod impls;
use impls::*;

#[proc_macro_derive(Serde)]
pub fn derive_serde(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let is_internal = false;
    derive_serde_common(input, is_internal)
}

#[proc_macro_derive(SerdeInternal)]
pub fn derive_serde_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let is_internal = true;
    derive_serde_common(input, is_internal)
}

fn derive_serde_common(
    input: proc_macro::TokenStream,
    is_internal: bool,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let input_name = input.ident;

    let gen = match &input.data {
        Data::Enum(enum_) => derive_serde_enum(enum_, &input_name, is_internal),
        Data::Struct(struct_) => match struct_.fields {
            Fields::Unit | Fields::Unnamed(_) => {
                derive_serde_tuple_struct(struct_, &input_name, is_internal)
            }
            Fields::Named(_) => derive_serde_struct(struct_, &input_name, is_internal),
        },
        _ => unimplemented!("Only structs and enums are supported"),
    };

    proc_macro::TokenStream::from(gen)
}
