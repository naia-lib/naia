use crate::parse::Struct;

#[allow(clippy::format_push_string)]
pub fn derive_serde_tuple_struct(struct_: &Struct) -> String {
    let mut ser_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(ser_body, "self.{}.ser(writer);", n);
    }

    let mut de_body = String::new();

    for (n, _) in struct_.fields.iter().enumerate() {
        l!(de_body, "{}: Serde::de(reader)?,", n);
    }

    let name = &struct_.name;

    format!(
        "
        mod impl_serde_{name} {{
            use super::serde::*;
            use super::{name};
            impl Serde for {name} {{
                fn ser(&self, writer: &mut dyn BitWrite) {{
                    {ser_body}
                }}
                fn de(reader: &mut BitReader) -> std::result::Result<Self, SerdeErr> {{
                    std::result::Result::Ok(Self {{
                        {de_body}
                    }})
                }}
            }}
        }}
        "
    )
}
