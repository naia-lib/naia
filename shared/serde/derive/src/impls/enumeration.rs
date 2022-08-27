use crate::parse::Enum;

fn bits_needed_for(max_value: usize) -> u8 {
    let mut bits = 1;
    while 2_usize.pow(bits) <= max_value {
        bits += 1;
    }
    if bits >= 256 {
        panic!("cannot encode a number in more than 255 bits!");
    }
    bits as u8
}

#[allow(clippy::format_push_string)]
pub fn derive_serde_enum(enum_: &Enum) -> String {
    let variant_number = enum_.variants.len();
    let bits_needed = bits_needed_for(variant_number);

    let mut ser_variants = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_name = &variant.name;

        // Unit Variant
        if variant.fields.is_empty() {
            l!(ser_variants, "Self::{} => {{", variant_name);

            // INDEX
            l!(
                ser_variants,
                "let index = UnsignedInteger::<{}>::new({});",
                bits_needed,
                index
            );
            l!(ser_variants, "index.ser(writer);");

            l!(ser_variants, "},");
        }
        // Struct Variant
        else if !variant.tuple {
            l!(ser_variants, "Self::{} {{", variant.name);
            for field in &variant.fields {
                l!(ser_variants, "{}, ", field.field_name.as_ref().unwrap());
            }
            l!(ser_variants, "} => {");

            // INDEX
            l!(
                ser_variants,
                "let index = UnsignedInteger::<{}>::new({});",
                bits_needed,
                index
            );
            l!(ser_variants, "index.ser(writer);");

            for field in &variant.fields {
                l!(
                    ser_variants,
                    "{}.ser(writer);",
                    field.field_name.as_ref().unwrap()
                );
            }
            l!(ser_variants, "}");
        }
        // Tuple Variant
        else if variant.tuple {
            l!(ser_variants, "Self::{} (", variant.name);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(ser_variants, "f{}, ", n);
            }
            l!(ser_variants, ") => {");

            // INDEX
            l!(
                ser_variants,
                "let index = UnsignedInteger::<{}>::new({});",
                bits_needed,
                index
            );
            l!(ser_variants, "index.ser(writer);");

            for (n, _) in variant.fields.iter().enumerate() {
                l!(ser_variants, "f{}.ser(writer);", n);
            }
            l!(ser_variants, "}");
        }
    }

    let mut de_variants = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_index = format!("{}u16", index);

        // Unit Variant
        if variant.fields.is_empty() {
            l!(de_variants, "{} => Self::{},", variant_index, variant.name);
        }
        // Struct Variant
        else if !variant.tuple {
            l!(
                de_variants,
                "{} => Self::{} {{",
                variant_index,
                variant.name
            );
            for field in &variant.fields {
                l!(
                    de_variants,
                    "{}: Serde::de(reader)?,",
                    field.field_name.as_ref().unwrap()
                );
            }
            l!(de_variants, "},");
        }
        // Tuple Variant
        else if variant.tuple {
            l!(de_variants, "{} => Self::{} (", variant_index, variant.name);
            for _ in &variant.fields {
                l!(de_variants, "Serde::de(reader)?,");
            }
            l!(de_variants, "),");
        }
    }

    let name = enum_.name.clone();

    format!(
        "
        mod impl_serde_{name} {{
            use super::serde::*;
            use super::{name};
            impl Serde for {name} {{
                fn ser(&self, writer: &mut dyn BitWrite) {{
                    match self {{
                      {ser_variants}
                    }}
                }}
                fn de(reader: &mut BitReader) -> std::result::Result<Self, SerdeErr> {{
                    let index: UnsignedInteger<{bits_needed}> = Serde::de(reader)?;
                    let index_u16: u16 = index.get() as u16;
                    Ok(match index_u16 {{
                        {de_variants}
                        _ => return std::result::Result::Err(SerdeErr{{}})
                    }})
                }}
            }}
        }}
        "
    )
    .parse()
    .unwrap()
}
