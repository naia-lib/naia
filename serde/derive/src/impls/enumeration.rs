use crate::parse::Enum;

use proc_macro::TokenStream;

pub fn derive_ser_enum(enum_: &Enum) -> TokenStream {
    let mut r = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let lit = format!("{}u16", index);
        let ident = &variant.name;
        // Unit Variant
        if variant.fields.len() == 0 {
            l!(r, "Self::{} => writer.write(&{}),", ident, lit);
        }
        // Struct Variant
        else if variant.tuple == false {
            l!(r, "Self::{} {{", variant.name);
            for field in &variant.fields {
                l!(r, "{}, ", field.field_name.as_ref().unwrap());
            }
            l!(r, "} => {");
            l!(r, "writer.write(&{});", lit);
            for field in &variant.fields {
                l!(r, "writer.write({});", field.field_name.as_ref().unwrap());
            }
            l!(r, "}");
        }
        // Tuple Variant
        else if variant.tuple == true {
            l!(r, "Self::{} (", variant.name);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(r, "f{}, ", n);
            }
            l!(r, ") => {");
            l!(r, "writer.write(&{});", lit);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(r, "writer.write(f{});", n);
            }
            l!(r, "}");
        }
    }

    format!(
        "impl Ser for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                match self {{
                  {}
                }}
            }}
        }}",
        enum_.name, r
    )
        .parse()
        .unwrap()
}

pub fn derive_de_enum(enum_: &Enum) -> TokenStream {
    let mut r = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let lit = format!("{}u16", index);

        // Unit Variant
        if variant.fields.len() == 0 {
            l!(r, "{} => Self::{},", lit, variant.name);
        }
        // Struct Variant
        else if variant.tuple == false {
            l!(r, "{} => Self::{} {{", lit, variant.name);
            for field in &variant.fields {
                l!(
                    r,
                    "{}: reader.read()?,",
                    field.field_name.as_ref().unwrap()
                );
            }
            l!(r, "},");
        }
        // Tuple Variant
        else if variant.tuple == true {
            l!(r, "{} => Self::{} (", lit, variant.name);
            for _ in &variant.fields {
                l!(r, "reader.read()?,");
            }
            l!(r, "),");
        }
    }

    format!(
        "impl  De for {} {{
            fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::DeErr> {{
                let id: u16 = reader.read()?;
                Ok(match id {{
                    {}
                    _ => return std::result::Result::Err(naia_serde::DeErr{{}})
                }})
            }}
        }}", enum_.name, r)
        .parse()
        .unwrap()
}