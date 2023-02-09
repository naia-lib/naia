use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::DataStruct;

#[allow(clippy::format_push_string)]
pub fn derive_serde_struct(
    struct_: &DataStruct,
    struct_name: &Ident,
    is_internal: bool,
) -> TokenStream {
    let mut ser_body = quote! {};
    let mut de_body = quote! {};

    for field in &struct_.fields {
        let field_name = field.ident.as_ref().expect("expected field to have a name");
        ser_body = quote! {
            #ser_body
            self.#field_name.ser(writer);
        };
        de_body = quote! {
            #de_body
            #field_name: Serde::de(reader)?,
        };
    }

    let lowercase_struct_name = Ident::new(
        struct_name.to_string().to_lowercase().as_str(),
        Span::call_site(),
    );
    let module_name = format_ident!("define_{}", lowercase_struct_name);

    let import_types = quote! { Serde, BitWrite, BitReader, SerdeErr };
    let imports = match is_internal {
        true => {
            quote! {
                use naia_serde::{#import_types};
            }
        }
        false => {
            quote! {
                use naia_shared::{#import_types};
            }
        }
    };

    quote! {
        mod #module_name {
            #imports
            use super::#struct_name;
            impl Serde for #struct_name {
                 fn ser(&self, writer: &mut dyn BitWrite) {
                    #ser_body
                 }
                 fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
                    Ok(Self {
                        #de_body
                    })
                 }
            }
        }
    }
}
