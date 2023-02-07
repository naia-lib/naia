use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, Index, LitStr, Member,
    PathArguments, Type,
};

use crate::shared::{get_struct_type, StructType};

const UNNAMED_FIELD_PREFIX: &'static str = "unnamed_field_";

pub struct NormalProperty {
    pub variable_name: Ident,
    pub inner_type: Type,
    pub uppercase_variable_name: Ident,
}

pub struct EntityProperty {
    pub variable_name: Ident,
    pub uppercase_variable_name: Ident,
}

pub struct NonReplicatedProperty {
    pub variable_name: Ident,
    pub field_type: Type,
}

#[allow(clippy::large_enum_variant)]
pub enum Property {
    Normal(NormalProperty),
    Entity(EntityProperty),
    NonReplicated(NonReplicatedProperty),
}

pub fn replicate_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Helper Properties
    let properties = properties(&input);
    let struct_type = get_struct_type(&input);

    // Names
    let replica_name = input.ident.clone();
    let enum_name = format_ident!("{}Property", replica_name);
    let module_name = format_ident!("define_{}", replica_name);
    let builder_name = format_ident!("{}Builder", replica_name);
    let replica_name_str = LitStr::new(&replica_name.to_string(), replica_name.span());

    // Definitions
    let property_enum_definition = property_enum(&enum_name, &properties);
    let diff_mask_size = {
        let len = properties.len();
        if len == 0 {
            0
        } else {
            ((len - 1) / 8) + 1
        }
    } as u8;

    // Methods
    let new_complete_method =
        new_complete_method(&replica_name, &enum_name, &properties, &struct_type);
    let create_builder_method = create_builder_method(&builder_name);
    let read_method = read_method(&replica_name, &enum_name, &properties, &struct_type);
    let read_create_update_method = read_create_update_method(&properties);

    let dyn_ref_method = dyn_ref_method();
    let dyn_mut_method = dyn_mut_method();
    let clone_method = clone_method(&replica_name, &properties, &struct_type);
    let mirror_method = mirror_method(&replica_name, &properties, &struct_type);
    let set_mutator_method = set_mutator_method(&properties, &struct_type);
    let read_apply_update_method = read_apply_update_method(&properties, &struct_type);
    let write_method = write_method(&properties, &struct_type);
    let write_update_method = write_update_method(&enum_name, &properties, &struct_type);
    let has_entity_properties = has_entity_properties_method(&properties);
    let entities = entities_method(&properties, &struct_type);

    let gen = quote! {
        mod #module_name {

            use std::{rc::Rc, cell::RefCell, io::Cursor, any::Any};
            use naia_shared::{
                DiffMask, PropertyMutate, PropertyMutator, ComponentUpdate,
                ReplicaDynRef, ReplicaDynMut, NetEntityHandleConverter, ComponentId, Named,
                BitReader, BitWrite, BitWriter, OwnedBitReader, SerdeErr, Serde,
                EntityProperty, EntityHandle, Replicate, Property, Components, ReplicateBuilder
            };
            use super::*;

            #property_enum_definition

            struct #builder_name {
                component_id: ComponentId,
            }
            impl ReplicateBuilder for #builder_name {
                #read_method
                #read_create_update_method
            }

            impl #replica_name {
                #new_complete_method
            }
            impl Named for #replica_name {
                fn name(&self) -> String {
                    return #replica_name_str.to_string();
                }
            }
            impl Replicate for #replica_name {
                fn to_any(&self) -> &dyn Any {
                    self
                }
                fn to_any_mut(&mut self) -> &mut dyn Any {
                    self
                }
                fn kind(&self) -> ComponentId {
                    Components::type_to_id::<#replica_name>()
                }
                fn diff_mask_size(&self) -> u8 { #diff_mask_size }
                fn copy_to_box(&self) -> Box<dyn Replicate> {
                    Box::new(self.clone())
                }
                #create_builder_method
                #dyn_ref_method
                #dyn_mut_method
                #mirror_method
                #set_mutator_method
                #write_method
                #write_update_method
                #read_apply_update_method
                #has_entity_properties
                #entities
            }
            impl Clone for #replica_name {
                #clone_method
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

/// Create a variable name for unnamed fields
fn get_variable_name_for_unnamed_field(index: usize, span: Span) -> Ident {
    Ident::new(&format!("{}{}", UNNAMED_FIELD_PREFIX, index), span)
}

/// Get the field name as a TokenStream
fn get_field_name(property: &Property, index: usize, struct_type: &StructType) -> Member {
    match *struct_type {
        StructType::Struct => Member::from(property.variable_name().clone()),
        StructType::TupleStruct => {
            let index = Index {
                index: index as u32,
                span: property.variable_name().span(),
            };
            Member::from(index)
        }
        _ => {
            panic!("The struct should not have any fields")
        }
    }
}

impl Property {
    pub fn normal(variable_name: Ident, inner_type: Type) -> Self {
        Self::Normal(NormalProperty {
            variable_name: variable_name.clone(),
            inner_type,
            uppercase_variable_name: Ident::new(
                variable_name.to_string().to_uppercase().as_str(),
                Span::call_site(),
            ),
        })
    }

    pub fn entity(variable_name: Ident) -> Self {
        Self::Entity(EntityProperty {
            variable_name: variable_name.clone(),
            uppercase_variable_name: Ident::new(
                variable_name.to_string().to_uppercase().as_str(),
                Span::call_site(),
            ),
        })
    }

    pub fn nonreplicated(variable_name: Ident, field_type: Type) -> Self {
        Self::NonReplicated(NonReplicatedProperty {
            variable_name: variable_name.clone(),
            field_type,
        })
    }

    pub fn is_replicated(&self) -> bool {
        match self {
            Self::Normal(_) | Self::Entity(_) => true,
            Self::NonReplicated(_) => false,
        }
    }

    pub fn variable_name(&self) -> &Ident {
        match self {
            Self::Normal(property) => &property.variable_name,
            Self::Entity(property) => &property.variable_name,
            Self::NonReplicated(property) => &property.variable_name,
        }
    }

    pub fn uppercase_variable_name(&self) -> &Ident {
        match self {
            Self::Normal(property) => &property.uppercase_variable_name,
            Self::Entity(property) => &property.uppercase_variable_name,
            Self::NonReplicated(_) => panic!("Unused for non-replicated properties"),
        }
    }
}

fn properties(input: &DeriveInput) -> Vec<Property> {
    let mut fields = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        match &data_struct.fields {
            Fields::Named(fields_named) => {
                for field in fields_named.named.iter() {
                    if let Some(variable_name) = &field.ident {
                        if let Type::Path(type_path) = &field.ty {
                            if let Some(property_seg) = type_path.path.segments.first() {
                                let property_type = property_seg.ident.clone();
                                // EntityProperty
                                if property_type == "EntityProperty" {
                                    fields.push(Property::entity(variable_name.clone()));
                                    continue;
                                // Property
                                } else if property_type == "Property" {
                                    if let PathArguments::AngleBracketed(angle_args) =
                                        &property_seg.arguments
                                    {
                                        if let Some(GenericArgument::Type(inner_type)) =
                                            angle_args.args.first()
                                        {
                                            fields.push(Property::normal(
                                                variable_name.clone(),
                                                inner_type.clone(),
                                            ));
                                            continue;
                                        }
                                    }
                                // Non-replicated Property
                                } else {
                                    fields.push(Property::nonreplicated(
                                        variable_name.clone(),
                                        field.ty.clone(),
                                    ));
                                }
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
                                fields.push(Property::entity(variable_name));
                                continue;
                            } else if let PathArguments::AngleBracketed(angle_args) =
                                &property_seg.arguments
                            {
                                if let Some(GenericArgument::Type(inner_type)) =
                                    angle_args.args.first()
                                {
                                    fields
                                        .push(Property::normal(variable_name, inner_type.clone()));
                                    continue;
                                }
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

fn property_enum(enum_name: &Ident, properties: &[Property]) -> TokenStream {
    if properties.is_empty() {
        return quote! {
            enum #enum_name {}
        };
    }

    let hashtag = Punct::new('#', Spacing::Alone);

    let mut variant_list = quote! {};

    for (index, property) in properties.iter().filter(|p| p.is_replicated()).enumerate() {
        let index = syn::Index::from(index);
        let uppercase_variant_name = property.uppercase_variable_name();

        let new_output_right = quote! {
            #uppercase_variant_name = #index as u8,
        };
        let new_output_result = quote! {
            #variant_list
            #new_output_right
        };
        variant_list = new_output_result;
    }

    quote! {
        #hashtag[repr(u8)]
        enum #enum_name {
            #variant_list
        }
    }
}

pub fn dyn_ref_method() -> TokenStream {
    quote! {
        fn dyn_ref(&self) -> ReplicaDynRef<'_> {
            return ReplicaDynRef::new(self);
        }
    }
}

pub fn dyn_mut_method() -> TokenStream {
    quote! {
        fn dyn_mut(&mut self) -> ReplicaDynMut<'_> {
            return ReplicaDynMut::new(self);
        }
    }
}

fn clone_method(
    replica_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut output = quote! {};
    let mut entity_property_output = quote! {};

    for (index, property) in properties.iter().enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        match property {
            Property::Normal(_) => {
                let new_output_right = quote! {
                    (*self.#field_name).clone(),
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            Property::NonReplicated(_) => {
                let new_output_right = quote! {
                    (self.#field_name).clone(),
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            Property::Entity(_) => {
                let new_output_right = quote! {
                    new_clone.#field_name.mirror(&self.#field_name);
                };
                let new_output_result = quote! {
                    #entity_property_output
                    #new_output_right
                };
                entity_property_output = new_output_result;
            }
        };
    }

    quote! {
        fn clone(&self) -> #replica_name {
            let mut new_clone = #replica_name::new_complete(#output);
            #entity_property_output
            return new_clone;
        }
    }
}

fn mirror_method(
    replica_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut output = quote! {};

    for (index, property) in properties.iter().filter(|p| p.is_replicated()).enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        let new_output_right = quote! {
            self.#field_name.mirror(&replica.#field_name);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    quote! {
        fn mirror(&mut self, other: &dyn Replicate) {
            if let Some(replica) = other.to_any().downcast_ref::<#replica_name>() {
                #output
            } else {
                panic!("cannot mirror: other Component is of another type!");
            }
        }
    }
}

fn set_mutator_method(properties: &[Property], struct_type: &StructType) -> TokenStream {
    let mut output = quote! {};

    for (index, property) in properties.iter().filter(|p| p.is_replicated()).enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        let new_output_right = quote! {
                self.#field_name.set_mutator(mutator);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    quote! {
        fn set_mutator(&mut self, mutator: &PropertyMutator) {
            #output
        }
    }
}

pub fn new_complete_method(
    replica_name: &Ident,
    enum_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut args = quote! {};
    for property in properties.iter() {
        match property {
            Property::Normal(property) => {
                let field_name = &property.variable_name;
                let field_type = &property.inner_type;

                let new_output_right = quote! {
                    #field_name: #field_type,
                };

                let new_output_result = quote! {
                    #args #new_output_right
                };
                args = new_output_result;
            }
            Property::NonReplicated(property) => {
                let field_name = &property.variable_name;
                let field_type = &property.field_type;

                let new_output_right = quote! {
                    #field_name: #field_type,
                };

                let new_output_result = quote! {
                    #args #new_output_right
                };
                args = new_output_result;
            }
            Property::Entity(_) => {
                continue;
            }
        };
    }

    let mut fields = quote! {};
    for property in properties.iter() {
        let new_output_right = match property {
            Property::Normal(property) => {
                let field_name = &property.variable_name;
                let field_type = &property.inner_type;
                let uppercase_variant_name = &property.uppercase_variable_name;

                match *struct_type {
                    StructType::Struct => {
                        quote! {
                            #field_name: Property::<#field_type>::new(#field_name, #enum_name::#uppercase_variant_name as u8)
                        }
                    }
                    StructType::TupleStruct => {
                        quote! {
                            Property::<#field_type>::new(#field_name, #enum_name::#uppercase_variant_name as u8)
                        }
                    }
                    _ => {
                        quote! {}
                    }
                }
            }
            Property::Entity(property) => {
                let field_name = &property.variable_name;
                let uppercase_variant_name = &property.uppercase_variable_name;

                match *struct_type {
                    StructType::Struct => {
                        quote! {
                             #field_name: EntityProperty::new(#enum_name::#uppercase_variant_name as u8)
                        }
                    }
                    StructType::TupleStruct => {
                        quote! {
                            EntityProperty::new(#enum_name::#uppercase_variant_name as u8)
                        }
                    }
                    _ => {
                        quote! {}
                    }
                }
            }
            Property::NonReplicated(property) => {
                let field_name = &property.variable_name;
                match *struct_type {
                    StructType::Struct => {
                        quote! {
                             #field_name: #field_name
                        }
                    }
                    StructType::TupleStruct => {
                        quote! {
                            #field_name
                        }
                    }
                    _ => {
                        quote! {}
                    }
                }
            }
        };

        let new_output_result = quote! {
            #fields
            #new_output_right,
        };
        fields = new_output_result;
    }

    let fn_inner = match *struct_type {
        StructType::Struct => {
            quote! {
                #replica_name {
                    #fields
                }
            }
        }
        StructType::TupleStruct => {
            quote! {
                #replica_name (
                    #fields
                )
            }
        }
        StructType::UnitStruct => {
            quote! {
                #replica_name
            }
        }
    };

    quote! {
        pub fn new_complete(#args) -> #replica_name {
            #fn_inner
        }
    }
}

pub fn create_builder_method(builder_name: &Ident) -> TokenStream {
    quote! {
        fn create_builder(component_id: ComponentId) -> Box<dyn ReplicateBuilder> where Self:Sized {
            Box::new(#builder_name { component_id })
        }
    }
}

pub fn read_method(
    replica_name: &Ident,
    enum_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut prop_names = quote! {};
    for property in properties.iter() {
        let field_name = property.variable_name();
        let new_output_right = quote! {
            #field_name
        };
        let new_output_result = quote! {
            #prop_names
            #new_output_right,
        };
        prop_names = new_output_result;
    }

    let mut prop_reads = quote! {};
    for property in properties.iter() {
        let field_name = property.variable_name();
        let new_output_right = match property {
            Property::Normal(property) => {
                let field_type = &property.inner_type;
                let uppercase_variant_name = &property.uppercase_variable_name;
                quote! {
                    let #field_name = Property::<#field_type>::new_read(reader, #enum_name::#uppercase_variant_name as u8)?;
                }
            }
            Property::Entity(property) => {
                let uppercase_variant_name = &property.uppercase_variable_name;
                quote! {
                    let #field_name = EntityProperty::new_read(reader, #enum_name::#uppercase_variant_name as u8, converter)?;
                }
            }
            Property::NonReplicated(property) => {
                let field_name = &property.variable_name;
                let field_type = &property.field_type;
                quote! {
                    let #field_name = <#field_type>::default();
                }
            }
        };

        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    let replica_build = match *struct_type {
        StructType::Struct => {
            quote! {
                #replica_name {
                    #prop_names
                }
            }
        }
        StructType::TupleStruct => {
            quote! {
                #replica_name (
                    #prop_names
                )
            }
        }
        StructType::UnitStruct => {
            quote! {
                #replica_name
            }
        }
    };

    quote! {
        fn read(&self, reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) -> Result<Box<dyn Replicate>, SerdeErr> {
            #prop_reads

            return Ok(Box::new(#replica_build));
        }
    }
}

pub fn read_create_update_method(properties: &[Property]) -> TokenStream {
    let mut prop_read_writes = quote! {};
    for property in properties.iter() {
        let new_output_right = match property {
            Property::Normal(property) => {
                let field_type = &property.inner_type;
                quote! {
                    {
                        let should_read = bool::de(reader)?;
                        should_read.ser(&mut update_writer);
                        if should_read {
                            Property::<#field_type>::read_write(reader, &mut update_writer)?;
                        }
                    }
                }
            }
            Property::Entity(_) => {
                quote! {
                    {
                        let should_read = bool::de(reader)?;
                        should_read.ser(&mut update_writer);
                        if should_read {
                            EntityProperty::read_write(reader, &mut update_writer)?;
                        }
                    }
                }
            }
            Property::NonReplicated(_) => {
                continue;
            }
        };

        let new_output_result = quote! {
            #prop_read_writes
            #new_output_right
        };
        prop_read_writes = new_output_result;
    }

    quote! {
        fn read_create_update(&self, reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr> {

            let mut update_writer = BitWriter::new();

            #prop_read_writes

            let (length, buffer) = update_writer.flush();
            let owned_reader = OwnedBitReader::new(&buffer[..length]);

            return Ok(ComponentUpdate::new(self.component_id, owned_reader));
        }
    }
}

fn read_apply_update_method(properties: &[Property], struct_type: &StructType) -> TokenStream {
    let mut output = quote! {};

    for (index, property) in properties.iter().enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        let new_output_right = match property {
            Property::Normal(_) => {
                quote! {
                    if bool::de(reader)? {
                        Property::read(&mut self.#field_name, reader)?;
                    }
                }
            }
            Property::Entity(_) => {
                quote! {
                    if bool::de(reader)? {
                        EntityProperty::read(&mut self.#field_name, reader, converter)?;
                    }
                }
            }
            Property::NonReplicated(_) => {
                continue;
            }
        };

        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    quote! {
        fn read_apply_update(&mut self, converter: &dyn NetEntityHandleConverter, mut update: ComponentUpdate) -> Result<(), SerdeErr> {
            let reader = &mut update.reader();
            #output
            Ok(())
        }
    }
}

fn write_method(properties: &[Property], struct_type: &StructType) -> TokenStream {
    let mut property_writes = quote! {};

    for (index, property) in properties.iter().enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        let new_output_right = match property {
            Property::Normal(_) => {
                quote! {
                    Property::write(&self.#field_name, bit_writer);
                }
            }
            Property::Entity(_) => {
                quote! {
                    EntityProperty::write(&self.#field_name, bit_writer, converter);
                }
            }
            Property::NonReplicated(_) => {
                continue;
            }
        };

        let new_output_result = quote! {
            #property_writes
            #new_output_right
        };
        property_writes = new_output_result;
    }

    quote! {
        fn write(&self, bit_writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
            self.kind().ser(bit_writer);
            #property_writes
        }
    }
}

fn write_update_method(
    enum_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut output = quote! {};

    for (index, property) in properties.iter().enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        let new_output_right = match property {
            Property::Normal(property) => {
                let uppercase_variant_name = &property.uppercase_variable_name;
                quote! {
                    if let Some(true) = diff_mask.bit(#enum_name::#uppercase_variant_name as u8) {
                        true.ser(writer);
                        Property::write(&self.#field_name, writer);
                    } else {
                        false.ser(writer);
                    }
                }
            }
            Property::Entity(property) => {
                let uppercase_variant_name = &property.uppercase_variable_name;
                quote! {
                    if let Some(true) = diff_mask.bit(#enum_name::#uppercase_variant_name as u8) {
                        true.ser(writer);
                        EntityProperty::write(&self.#field_name, writer, converter);
                    } else {
                        false.ser(writer);
                    }
                }
            }
            Property::NonReplicated(_) => {
                continue;
            }
        };

        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    quote! {
        fn write_update(&self, diff_mask: &DiffMask, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
            #output
        }
    }
}

fn has_entity_properties_method(properties: &[Property]) -> TokenStream {
    for property in properties.iter() {
        if let Property::Entity(_) = property {
            return quote! {
                fn has_entity_properties(&self) -> bool {
                    return true;
                }
            };
        }
    }

    quote! {
        fn has_entity_properties(&self) -> bool {
            return false;
        }
    }
}

fn entities_method(properties: &[Property], struct_type: &StructType) -> TokenStream {
    let mut body = quote! {};

    for (index, property) in properties.iter().enumerate() {
        if let Property::Entity(_) = property {
            let field_name = get_field_name(property, index, struct_type);
            let body_add_right = quote! {
                if let Some(handle) = self.#field_name.handle() {
                    output.push(handle);
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
        fn entities(&self) -> Vec<EntityHandle> {
            let mut output = Vec::new();
            #body
            return output;
        }
    }
}
