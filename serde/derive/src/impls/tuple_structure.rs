use crate::parse::Struct;

use proc_macro::TokenStream;

pub fn derive_ser_tuple_struct(struct_: &Struct) -> TokenStream {
    let mut body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(body, "writer.write(&self.{});", n);
    }
    format!(
        "impl Ser for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                {}
            }}
        }}",
        struct_.name, body
    )
        .parse()
        .unwrap()
}

pub fn derive_de_tuple_struct(struct_: &Struct) -> TokenStream {
    let mut body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(body, "{}: reader.read()?,", n);
    }

    format!(
        "impl De for {} {{
            fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::DeErr> {{
                std::result::Result::Ok(Self {{
                    {}
                }})
            }}
        }}",
        struct_.name, body
    )
        .parse()
        .unwrap()
}