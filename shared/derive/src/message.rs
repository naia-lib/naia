use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, Index, LitStr, Member, Type};

use super::shared::{get_struct_type, StructType};

pub fn message_impl(
    input: proc_macro::TokenStream,
    shared_crate_name: TokenStream,
    is_fragment: bool,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Helper Properties
    let struct_type = get_struct_type(&input);
    let fields = get_fields(&input);

    // Names
    let struct_name = input.ident;
    let struct_name_str = LitStr::new(&struct_name.to_string(), struct_name.span());
    let lowercase_struct_name = Ident::new(
        struct_name.to_string().to_lowercase().as_str(),
        Span::call_site(),
    );
    let module_name = format_ident!("define_{}", lowercase_struct_name);
    let builder_name = format_ident!("{}Builder", struct_name);

    // Methods
    let clone_method = get_clone_method(&fields, &struct_type);
    // let has_entity_propertys_method = get_has_entity_propertys_method(&fields);
    // let entities_method = get_entities_method(&fields, &struct_type);
    let relations_waiting_method = get_relations_waiting_method(&fields, &struct_type);
    let relations_complete_method = get_relations_complete_method(&fields, &struct_type);
    let bit_length_method = get_bit_length_method(&fields, &struct_type);
    let write_method = get_write_method(&fields, &struct_type);
    let create_builder_method = get_create_builder_method(&builder_name);
    let read_method = get_read_method(&struct_name, &fields, &struct_type);
    let is_fragment_method = get_is_fragment_method(is_fragment);

    let gen = quote! {
        mod #module_name {

            pub use std::any::Any;
            pub use std::collections::HashSet;
            pub use #shared_crate_name::{
                Named, GlobalEntity, Message, BitWrite, LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut, LocalEntity,
                EntityProperty, MessageKind, MessageKinds, Serde, MessageBuilder, BitReader, SerdeErr, ConstBitLength, MessageContainer
            };
            use super::*;

            struct #builder_name;
            impl MessageBuilder for #builder_name {
                #read_method
            }

            impl Message for #struct_name {
                fn kind(&self) -> MessageKind {
                    MessageKind::of::<#struct_name>()
                }
                fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> {
                    self
                }
                #is_fragment_method
                #bit_length_method
                #create_builder_method
                #relations_waiting_method
                #relations_complete_method
                #write_method
            }
            impl Named for #struct_name {
                fn name(&self) -> String {
                    return #struct_name_str.to_string();
                }
            }
            impl Clone for #struct_name {
                #clone_method
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_is_fragment_method(is_fragment: bool) -> TokenStream {
    let value = {
        if is_fragment {
            quote! { true }
        } else {
            quote! { false }
        }
    };
    quote! {
        fn is_fragment(&self) -> bool {
            #value
        }
    }
}

fn get_clone_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
    let mut output = quote! {};

    for (index, field) in fields.iter().enumerate() {
        let field_name = get_field_name(field, index, struct_type);
        match field {
            Field::Normal(_) | Field::EntityProperty(_) => {
                let new_output_right = quote! {
                    #field_name: self.#field_name.clone(),
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
        };
    }

    quote! {
        fn clone(&self) -> Self {
            let mut new_clone = Self {
                #output
            };
            return new_clone;
        }
    }
}

// fn get_has_entity_propertys_method(fields: &[Field]) -> TokenStream {
//     for field in fields.iter() {
//         if let Field::EntityProperty(_) = field {
//             return quote! {
//                 fn has_entity_propertys(&self) -> bool {
//                     return true;
//                 }
//             };
//         }
//     }
//
//     quote! {
//         fn has_entity_propertys(&self) -> bool {
//             return false;
//         }
//     }
// }

// fn get_entities_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
//     let mut body = quote! {};
//
//     for (index, field) in fields.iter().enumerate() {
//         if let Field::EntityProperty(_) = field {
//             let field_name = get_field_name(field, index, struct_type);
//             let body_add_right = quote! {
//                 if let Some(global_entity) = self.#field_name.global_entity() {
//                     output.push(global_entity);
//                 }
//             };
//             let new_body = quote! {
//                 #body
//                 #body_add_right
//             };
//             body = new_body;
//         }
//     }
//
//     quote! {
//         fn entities(&self) -> Vec<GlobalEntity> {
//             let mut output = Vec::new();
//             #body
//             return output;
//         }
//     }
// }

fn get_relations_waiting_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
    let mut body = quote! {};

    for (index, field) in fields.iter().enumerate() {
        if let Field::EntityProperty(_) = field {
            let field_name = get_field_name(field, index, struct_type);
            let body_add_right = quote! {
                if let Some(local_entity) = self.#field_name.waiting_local_entity() {
                    output.insert(local_entity);
                }
            };
            let new_body = quote! {
                #body
                #body_add_right
            };
            body = new_body;
        }
    }

    quote! {
        fn relations_waiting(&self) -> Option<HashSet<LocalEntity>> {
            let mut output = HashSet::new();
            #body
            if output.is_empty() {
                return None;
            }
            return Some(output);
        }
    }
}

fn get_relations_complete_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
    let mut body = quote! {};

    for (index, field) in fields.iter().enumerate() {
        if let Field::EntityProperty(_) = field {
            let field_name = get_field_name(field, index, struct_type);
            let body_add_right = quote! {
                self.#field_name.waiting_complete(converter);
            };
            let new_body = quote! {
                #body
                #body_add_right
            };
            body = new_body;
        }
    }

    quote! {
        fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
            #body
        }
    }
}

pub fn get_read_method(
    struct_name: &Ident,
    fields: &[Field],
    struct_type: &StructType,
) -> TokenStream {
    let mut field_names = quote! {};
    for field in fields.iter() {
        let field_name = field.variable_name();
        let new_output_right = quote! {
            #field_name
        };
        let new_output_result = quote! {
            #field_names
            #new_output_right,
        };
        field_names = new_output_result;
    }

    let mut field_reads = quote! {};
    for field in fields.iter() {
        let field_name = field.variable_name();
        let new_output_right = match field {
            Field::EntityProperty(_property) => {
                quote! {
                    let #field_name = EntityProperty::new_read(reader, converter)?;
                }
            }
            Field::Normal(normal_field) => {
                let field_name = &normal_field.variable_name;
                let field_type = &normal_field.field_type;
                quote! {
                    let #field_name = <#field_type>::de(reader)?;
                }
            }
        };

        let new_output_result = quote! {
            #field_reads
            #new_output_right
        };
        field_reads = new_output_result;
    }

    let struct_build = match *struct_type {
        StructType::Struct => {
            quote! {
                #struct_name {
                    #field_names
                }
            }
        }
        StructType::TupleStruct => {
            quote! {
                #struct_name (
                    #field_names
                )
            }
        }
        StructType::UnitStruct => {
            quote! {
                #struct_name
            }
        }
    };

    quote! {
        fn read(&self, reader: &mut BitReader, converter: &dyn LocalEntityAndGlobalEntityConverter) -> Result<MessageContainer, SerdeErr> {
            #field_reads

            return Ok(MessageContainer::from_read(Box::new(#struct_build)));
        }
    }
}

fn get_write_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
    let mut field_writes = quote! {};

    for (index, field) in fields.iter().enumerate() {
        let field_name = get_field_name(field, index, struct_type);
        let new_output_right = match field {
            Field::Normal(_) => {
                quote! {
                    self.#field_name.ser(writer);
                }
            }
            Field::EntityProperty(_) => {
                quote! {
                    EntityProperty::write(&self.#field_name, writer, converter);
                }
            }
        };

        let new_output_result = quote! {
            #field_writes
            #new_output_right
        };
        field_writes = new_output_result;
    }

    quote! {
        fn write(&self, message_kinds: &MessageKinds, writer: &mut dyn BitWrite, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) {
            self.kind().ser(message_kinds, writer);
            #field_writes
        }
    }
}

fn get_bit_length_method(fields: &[Field], struct_type: &StructType) -> TokenStream {
    let mut field_bit_lengths = quote! {};

    for (index, field) in fields.iter().enumerate() {
        let field_name = get_field_name(field, index, struct_type);
        let new_output_right = match field {
            Field::Normal(_) => {
                quote! {
                    output += self.#field_name.bit_length();
                }
            }
            Field::EntityProperty(_) => {
                quote! {
                    output += self.#field_name.bit_length(converter);
                }
            }
        };

        let new_output_result = quote! {
            #field_bit_lengths
            #new_output_right
        };
        field_bit_lengths = new_output_result;
    }

    quote! {
        fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
            let mut output = 0;
            output += <MessageKind as ConstBitLength>::const_bit_length();
            #field_bit_lengths
            output
        }
    }
}

pub fn get_create_builder_method(builder_name: &Ident) -> TokenStream {
    quote! {
        fn create_builder() -> Box<dyn MessageBuilder> where Self:Sized {
            Box::new(#builder_name)
        }
    }
}

fn get_fields(input: &DeriveInput) -> Vec<Field> {
    let mut fields = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        match &data_struct.fields {
            Fields::Named(fields_named) => {
                for field in fields_named.named.iter() {
                    if let Some(variable_name) = &field.ident {
                        match &field.ty {
                            Type::Path(type_path) => {
                                if let Some(property_seg) = type_path.path.segments.first() {
                                    let property_type = property_seg.ident.clone();
                                    // EntityProperty
                                    if property_type == "EntityProperty" {
                                        fields.push(Field::entity_property(variable_name.clone()));
                                        continue;
                                        // Property
                                    } else {
                                        fields.push(Field::normal(
                                            variable_name.clone(),
                                            field.ty.clone(),
                                        ));
                                    }
                                }
                            }
                            _ => {
                                fields.push(Field::normal(variable_name.clone(), field.ty.clone()));
                            }
                        }
                    }
                }
            }
            Fields::Unnamed(fields_unnamed) => {
                for (index, field) in fields_unnamed.unnamed.iter().enumerate() {
                    if let Type::Path(type_path) = &field.ty {
                        if let Some(property_seg) = type_path.path.segments.first() {
                            let property_type = property_seg.ident.clone();
                            let variable_name =
                                get_variable_name_for_unnamed_field(index, property_type.span());
                            if property_type == "EntityProperty" {
                                fields.push(Field::entity_property(variable_name));
                                continue;
                            } else {
                                fields.push(Field::normal(variable_name, field.ty.clone()))
                            }
                        }
                    }
                }
            }
            Fields::Unit => {}
        }
    } else {
        panic!("Can only derive Replicate on a struct");
    }

    fields
}

/// Get the field name as a TokenStream
fn get_field_name(field: &Field, index: usize, struct_type: &StructType) -> Member {
    match *struct_type {
        StructType::Struct => Member::from(field.variable_name().clone()),
        StructType::TupleStruct => {
            let index = Index {
                index: index as u32,
                span: field.variable_name().span(),
            };
            Member::from(index)
        }
        _ => {
            panic!("The struct should not have any fields")
        }
    }
}

const UNNAMED_FIELD_PREFIX: &'static str = "unnamed_field_";
fn get_variable_name_for_unnamed_field(index: usize, span: Span) -> Ident {
    Ident::new(&format!("{}{}", UNNAMED_FIELD_PREFIX, index), span)
}

pub struct EntityProperty {
    pub variable_name: Ident,
    pub uppercase_variable_name: Ident,
}

pub struct Normal {
    pub variable_name: Ident,
    pub field_type: Type,
}

#[allow(clippy::large_enum_variant)]
pub enum Field {
    EntityProperty(EntityProperty),
    Normal(Normal),
}

impl Field {
    pub fn entity_property(variable_name: Ident) -> Self {
        Self::EntityProperty(EntityProperty {
            variable_name: variable_name.clone(),
            uppercase_variable_name: Ident::new(
                variable_name.to_string().to_uppercase().as_str(),
                Span::call_site(),
            ),
        })
    }

    pub fn normal(variable_name: Ident, field_type: Type) -> Self {
        Self::Normal(Normal {
            variable_name: variable_name.clone(),
            field_type,
        })
    }

    pub fn variable_name(&self) -> &Ident {
        match self {
            Self::EntityProperty(property) => &property.variable_name,
            Self::Normal(field) => &field.variable_name,
        }
    }
}
