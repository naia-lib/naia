extern crate proc_macro;

#[macro_use]
mod shared;

mod parse;
mod impls;

use crate::impls::*;

#[proc_macro_derive(Serde)]
pub fn derive_serde(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse::parse_data(input);

    // ok we have an ident, its either a struct or a enum
    let ts = match &input {
        parse::Data::Struct(struct_) if struct_.tuple => derive_serde_tuple_struct(struct_),
        parse::Data::Struct(struct_) => derive_serde_struct(struct_),
        parse::Data::Enum(enum_) => derive_serde_enum(enum_),
        _ => unimplemented!("Only structs and enums are supported"),
    };

    ts
}