//! Gatt Service Builder
//!
//! This module contains the ServiceBuilder struct which is used to construct a Gatt Service from a struct definition.
//! The struct definition is used to define the characteristics of the service, and the ServiceBuilder is used to
//! generate the code required to create the service.

use darling::Error;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::parse::Result;
use syn::spanned::Spanned;
use syn::{Meta, Token};

use crate::characteristic::{Characteristic, PropertiesArgs};
use crate::uuid::parse_arg_uuid;

#[derive(Debug)]
pub(crate) struct ServiceArgs {
    pub uuid: TokenStream2,
}

impl syn::parse::Parse for ServiceArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut uuid: Option<_> = None;

        while !input.is_empty() {
            let meta = input.parse()?;

            match &meta {
                Meta::NameValue(name_value) => {
                    match name_value
                        .path
                        .get_ident()
                        .ok_or(Error::custom("Argument name is missing").with_span(&name_value.span()))?
                        .to_string()
                        .as_str()
                    {
                        "uuid" => {
                            if uuid.is_some() {
                                return Err(Error::custom("UUID cannot be specified more than once")
                                    .with_span(&name_value.span())
                                    .into());
                            }
                            uuid = Some(parse_arg_uuid(&name_value.value)?);
                        }
                        other => {
                            return Err(Error::unknown_field(&format!(
                                "Unsupported service property: '{other}'.\nSupported properties are: uuid"
                            ))
                            .with_span(&name_value.span())
                            .into())
                        }
                    }
                }
                _ => return Err(Error::custom("Unexpected argument").with_span(&meta.span()).into()),
            }
            let _ = input.parse::<Token![,]>();
        }

        Ok(Self {
            uuid: uuid.ok_or(Error::custom(
                "Service must have a UUID (i.e. `#[gatt_service(uuid = '1234')]` or `#[gatt_service(uuid = service::BATTERY)]`)",
            ))?,
        })
    }
}

pub(crate) struct ServiceBuilder {
    properties: syn::ItemStruct,
    args: ServiceArgs,
    code_attribute_count: TokenStream2,
    code_cccd_count: TokenStream2,
    code_impl: TokenStream2,
    code_build_chars: TokenStream2,
    code_struct_init: TokenStream2,
    code_fields: TokenStream2,
    code_errors: TokenStream2,
}

impl ServiceBuilder {
    pub fn new(properties: syn::ItemStruct, args: ServiceArgs) -> Self {
        Self {
            properties,
            args,
            code_attribute_count: TokenStream2::new(),
            code_cccd_count: TokenStream2::new(),
            code_struct_init: TokenStream2::new(),
            code_impl: TokenStream2::new(),
            code_fields: TokenStream2::new(),
            code_build_chars: TokenStream2::new(),
            code_errors: TokenStream2::new(),
        }
    }

    /// Count the attributes a characteristic adds to the table, under its `cfg` if it has one.
    ///
    /// At least two attributes will be added to the attribute table for each characteristic:
    /// - The characteristic declaration
    /// - The characteristic's value declaration
    ///
    /// If the characteristic has either the notify or indicate property,
    /// a Client Characteristic Configuration Descriptor (CCCD) declaration will also be added,
    /// and each descriptor adds one more.
    fn count_attributes(&mut self, characteristic: &Characteristic) {
        let properties = &characteristic.args.properties;
        let has_cccd = properties.notify || properties.indicate;
        let attributes = 2 + usize::from(has_cccd) + characteristic.args.descriptors.len();
        let cccds = usize::from(has_cccd);
        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_attribute_count.extend(quote_spanned! {characteristic.span=>
            #(#cfg_attr)* {
                count += #attributes;
            }
        });
        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_cccd_count.extend(quote_spanned! {characteristic.span=>
            #(#cfg_attr)* {
                count += #cccds;
            }
        });
    }

    /// Construct the macro blueprint for the service struct.
    ///
    /// BattleTime patch: the service adds its attributes to a table built in a constant, so the table
    /// stays in flash; see third_party/trouble-host/BATTLETIME.md.
    pub fn build(self) -> TokenStream2 {
        let properties = self.properties;
        let visibility = &properties.vis;
        let struct_name = &properties.ident;
        let code_struct_init = self.code_struct_init;
        let code_impl = self.code_impl;
        let fields = self.code_fields;
        let code_build_chars = self.code_build_chars;
        let code_attribute_count = self.code_attribute_count;
        let code_cccd_count = self.code_cccd_count;
        let code_errors = self.code_errors;
        let uuid = self.args.uuid;
        quote! {
            #code_errors

            #visibility struct #struct_name {
                #fields
                handle: u16,
            }

            // CCCD_COUNT adds 0 for each characteristic without notify or indicate.
            #[allow(unused, clippy::identity_op)]
            impl #struct_name {
                #visibility const ATTRIBUTE_COUNT: usize = {
                    let mut count = 1usize;
                    #code_attribute_count
                    count
                };
                #visibility const CCCD_COUNT: usize = {
                    let mut count = 0usize;
                    #code_cccd_count
                    count
                };

                /// Adds the service's attributes to a table built in a constant, and returns its handles.
                #visibility const fn build<const MAX_ATTRIBUTES: usize>(
                    builder: &mut trouble_host::attribute::AttributeTableBuilder<MAX_ATTRIBUTES>,
                ) -> Self {
                    let handle = builder.add_service(#uuid);
                    #code_build_chars

                    Self {
                        handle,
                        #code_struct_init
                    }
                }

                #visibility fn handle_range(&self) -> core::ops::Range<u16> {
                    self.handle..(self.handle + Self::ATTRIBUTE_COUNT as u16)
                }

                #code_impl
            }
        }
    }

    /// Construct instructions for adding a characteristic whose value is kept in RAM.
    fn construct_characteristic_static(&mut self, characteristic: Characteristic) {
        let (code_descriptors, named_descriptors) = self.build_descriptors(&characteristic);
        let char_name = format_ident!("{}", characteristic.name);
        let ty = characteristic.ty;
        let properties = set_properties(&characteristic.args.properties);
        let read = characteristic.args.permissions.read;
        let write = characteristic.args.permissions.write;
        let cccd = characteristic.args.permissions.cccd;
        let uuid = characteristic.args.uuid;
        let default_value = match characteristic.args.default_value {
            Some(val) => quote!(#val),                                       // if set by user
            None => quote_spanned!(characteristic.span => <#ty>::default()), // or default otherwise
        };

        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_build_chars.extend(quote_spanned! {characteristic.span=>
            #(#cfg_attr)*
            let (#char_name, #(#named_descriptors),*) = {
                let characteristic = builder.add_characteristic::<#ty>(
                    #uuid,
                    trouble_host::attribute::CharacteristicProps::from_bits(0u8 #(| (#properties as u8))*),
                    trouble_host::attribute::AttPermissions::new(#read, #write),
                    #cccd,
                    Some({
                        fn initial_value(buffer: &mut [u8]) -> usize {
                            trouble_host::attribute::write_initial_value::<#ty>(buffer, &#default_value)
                        }
                        initial_value
                    }),
                );
                #code_descriptors

                (builder.finish_characteristic(characteristic), #(#named_descriptors),*)
            };
        });

        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_struct_init.extend(quote_spanned!(characteristic.span=>
            #(#cfg_attr)*
            #char_name,
        ));
    }

    /// Construct instructions for adding a read-only characteristic whose value stays in flash.
    fn construct_characteristic_ro(&mut self, characteristic: Characteristic, value_type: syn::Type) {
        let (code_descriptors, named_descriptors) = self.build_descriptors(&characteristic);
        let char_name = format_ident!("{}", characteristic.name);
        let read = characteristic.args.permissions.read;
        let uuid = characteristic.args.uuid;
        let default_value = match characteristic.args.default_value {
            Some(val) => quote!(#val),                                               // if set by user
            None => quote_spanned!(characteristic.span => <#value_type>::default()), // or default otherwise
        };

        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_build_chars.extend(quote_spanned! {characteristic.span=>
            #(#cfg_attr)*
            let (#char_name, #(#named_descriptors),*) = {
                let characteristic = builder.add_constant_characteristic::<#value_type>(#uuid, #read, {
                    fn constant_value() -> &'static [u8] {
                        <#value_type as trouble_host::types::gatt_traits::AsGatt>::as_gatt(#default_value)
                    }
                    constant_value
                });
                #code_descriptors

                (builder.finish_characteristic(characteristic), #(#named_descriptors),*)
            };
        });

        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
        self.code_struct_init.extend(quote_spanned!(characteristic.span=>
            #(#cfg_attr)*
            #char_name,
        ));
    }

    /// Consume the lists of fields and fields marked as characteristics and prepare the code to add them to the service
    /// by generating the macro blueprints for any methods, fields, and static storage required.
    pub fn process_characteristics_and_fields(
        mut self,
        mut fields: Vec<syn::Field>,
        characteristics: Vec<Characteristic>,
    ) -> Self {
        // BattleTime patch: a service is built in a constant, so it can hold only characteristics.
        for field in &fields {
            self.code_errors.extend(
                syn::Error::new_spanned(
                    field,
                    "a gatt_service is built in a constant, so it can hold only characteristics",
                )
                .to_compile_error(),
            );
        }
        fields.clear();
        let mut attrs: Vec<Vec<syn::Attribute>> = Vec::new();
        // Process characteristic fields
        for ch in characteristics {
            let char_name = format_ident!("{}", ch.name);

            let mut ty = &ch.ty;
            let mut ro = false;
            if let syn::Type::Reference(type_ref) = ty {
                if ch.args.permissions.is_read_only()
                    && type_ref.mutability.is_none()
                    && type_ref.lifetime.as_ref().is_some_and(|lt| lt.ident == "static")
                {
                    ty = &type_ref.elem;
                    ro = true;
                }
            }
            let value_type = ty.clone();

            // add fields for each characteristic value handle
            fields.push(syn::Field {
                ident: Some(char_name.clone()),
                ty: syn::Type::Verbatim(quote!(trouble_host::attribute::Characteristic<#ty>)),
                attrs: Vec::new(),
                colon_token: Default::default(),
                vis: ch.vis.clone(),
                mutability: syn::FieldMutability::None,
            });

            let mut ch_attrs = ch.args.doc_string.clone();
            if let Some(cfg) = &ch.args.cfg {
                ch_attrs.push(cfg.clone());
            }
            attrs.push(ch_attrs);

            self.count_attributes(&ch);

            if ro {
                self.construct_characteristic_ro(ch, value_type);
            } else {
                self.construct_characteristic_static(ch);
            }
        }
        assert_eq!(fields.len(), attrs.len());
        // Processing common to all fields
        for (field, attrs) in fields.iter().zip(attrs) {
            let ident = field.ident.clone();
            let ty = field.ty.clone();
            let vis = &field.vis;
            self.code_fields.extend(quote_spanned! {field.span()=>
                #(#attrs)*
                #vis #ident: #ty,
            })
        }
        self
    }

    /// Generate token stream for any descriptors tagged against this characteristic.
    fn build_descriptors(&mut self, characteristic: &Characteristic) -> (TokenStream2, Vec<TokenStream2>) {
        let mut named_descriptors = Vec::<TokenStream2>::new();
        (
            characteristic
                .args
                .descriptors
                .iter()
                .map(|args| {
                    let identifier = args
                        .name
                        .as_ref()
                        .map(|name| format_ident!("{}_{}_descriptor", characteristic.name.as_str(), name.value()));
                    let read = args.permissions.read;
                    let write = args.permissions.write;
                    let uuid = &args.uuid;
                    let default_value = match &args.default_value {
                        Some(val) => quote!(#val), // if set by user
                        None => quote!(""),
                    };

                    let mut ty = args.ty.as_ref();
                    let mut ro = false;
                    if let Some(syn::Type::Reference(type_ref)) = args.ty.as_ref() {
                        if args.permissions.is_read_only()
                            && type_ref.mutability.is_none()
                            && type_ref.lifetime.as_ref().is_some_and(|lt| lt.ident == "static")
                        {
                            ty = Some(&type_ref.elem);
                            ro = true;
                        }
                    }

                    let mut identifier_assignment = None;
                    if let Some(name) = &identifier {
                        let ty = ty.unwrap(); // The type is required for named descriptors
                        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
                        self.code_fields.extend(quote_spanned! { identifier.span() =>
                            #(#cfg_attr)*
                            #name: trouble_host::attribute::Descriptor<#ty>,
                        });
                        let cfg_attr = characteristic.args.cfg.as_ref().into_iter();
                        self.code_struct_init.extend(quote_spanned! { identifier.span() =>
                            #(#cfg_attr)*
                            #name,
                        });
                        named_descriptors.push(name.to_token_stream());
                        identifier_assignment = Some(quote! { let #name = });
                    };

                    if ro {
                        let value_type = ty.unwrap();
                        quote_spanned! {characteristic.span=>
                            #identifier_assignment builder.add_constant_descriptor::<#value_type>(#uuid, #read, {
                                fn constant_value() -> &'static [u8] {
                                    <#value_type as trouble_host::types::gatt_traits::AsGatt>::as_gatt(#default_value)
                                }
                                constant_value
                            });
                        }
                    } else {
                        let permissions = quote!(trouble_host::attribute::AttPermissions::new(#read, #write));
                        match ty {
                            Some(ty) => quote_spanned! {characteristic.span=>
                                #identifier_assignment builder.add_descriptor::<#ty>(
                                    #uuid,
                                    #permissions,
                                    <#ty as trouble_host::types::gatt_traits::AsGatt>::MAX_SIZE,
                                    Some({
                                        fn initial_value(buffer: &mut [u8]) -> usize {
                                            trouble_host::attribute::write_initial_value::<#ty>(buffer, &#default_value)
                                        }
                                        initial_value
                                    }),
                                );
                            },
                            None => quote_spanned! {characteristic.span=>
                                #identifier_assignment builder.add_descriptor_like(
                                    &#default_value,
                                    #uuid,
                                    #permissions,
                                    #default_value.len(),
                                    Some({
                                        fn initial_value(buffer: &mut [u8]) -> usize {
                                            trouble_host::attribute::write_initial_value(buffer, &#default_value)
                                        }
                                        initial_value
                                    }),
                                );
                            },
                        }
                    }
                })
                .collect(),
            named_descriptors,
        )
    }
}

fn parse_property_into_list(property: bool, variant: TokenStream2, properties: &mut Vec<TokenStream2>) {
    if property {
        properties.push(variant);
    }
}

/// Parse the properties of a characteristic and return a list of properties
fn set_properties(args: &PropertiesArgs) -> Vec<TokenStream2> {
    let mut properties = Vec::new();
    parse_property_into_list(
        args.read,
        quote! {trouble_host::attribute::CharacteristicProp::Read},
        &mut properties,
    );
    parse_property_into_list(
        args.write,
        quote! {trouble_host::attribute::CharacteristicProp::Write},
        &mut properties,
    );
    parse_property_into_list(
        args.write_without_response,
        quote! {trouble_host::attribute::CharacteristicProp::WriteWithoutResponse},
        &mut properties,
    );
    parse_property_into_list(
        args.notify,
        quote! {trouble_host::attribute::CharacteristicProp::Notify},
        &mut properties,
    );
    parse_property_into_list(
        args.indicate,
        quote! {trouble_host::attribute::CharacteristicProp::Indicate},
        &mut properties,
    );
    properties
}
