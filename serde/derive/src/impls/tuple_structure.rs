use crate::parse::Struct;

pub fn derive_serde_tuple_struct(struct_: &Struct) -> String {
    let mut ser_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(ser_body, "self.{}.ser(writer);", n);
    }

    let mut de_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(de_body, "{}: Serde::de(reader)?,", n);
    }

    format!(
        "impl Serde for {} {{
            fn ser<S: BitWrite>(&self, writer: &mut S) {{
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
