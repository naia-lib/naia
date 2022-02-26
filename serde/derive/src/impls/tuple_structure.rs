use crate::parse::Struct;

pub fn derive_serde_tuple_struct(struct_: &Struct) -> String {
    let mut ser_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(ser_body, "writer.write(&self.{});", n);
    }

    let mut de_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(de_body, "{}: reader.read()?,", n);
    }

    format!(
        "impl Serde for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                {}
            }}
            fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::SerdeErr> {{
                std::result::Result::Ok(Self {{
                    {}
                }})
            }}
        }}",
        struct_.name, ser_body, de_body,
    )
}