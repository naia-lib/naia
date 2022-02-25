extern crate proc_macro;

#[macro_use]
mod shared;

mod parse;
mod impls;

use crate::impls::*;

#[proc_macro_derive(Ser)]
pub fn derive_ser(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse::parse_data(input);

    // ok we have an ident, its either a struct or a enum
    let ts = match &input {
        parse::Data::Struct(struct_) if struct_.tuple => derive_ser_tuple_struct(struct_),
        parse::Data::Struct(struct_) => derive_ser_struct(struct_),
        //parse::Data::Enum(enum_) => derive_ser_enum(enum_),
        _ => unimplemented!("Only structs and enums are supported"),
    };

    ts
}

#[proc_macro_derive(De)]
pub fn derive_de(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse::parse_data(input);

    // ok we have an ident, its either a struct or a enum
    let ts = match &input {
        parse::Data::Struct(struct_) if struct_.tuple => derive_de_tuple_struct(struct_),
        parse::Data::Struct(struct_) => derive_de_struct(struct_),
        //parse::Data::Enum(enum_) => derive_de_enum(enum_),

        _ => unimplemented!("Only structs and enums are supported"),
    };

    ts
}