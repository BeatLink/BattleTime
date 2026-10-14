//! Gatt Server Builder
//!
//! This module is responsible for generating the Gatt Server struct and its implementation.
//! It should contain one or more Gatt Services, which are decorated with the `#[gatt_service(uuid = "...")]` attribute.

use darling::Error;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{parse_quote, Expr, Result};

#[derive(Default)]
pub(crate) struct ServerArgs {
    mutex_type: Option<syn::Type>,
    packet_type: Option<syn::Type>,
    attribute_table_size: Option<Expr>,
    connections_max: Option<Expr>,
}

impl ServerArgs {
    pub fn parse(&mut self, meta: ParseNestedMeta) -> Result<()> {
        match meta
            .path
            .get_ident()
            .ok_or(Error::custom("no ident"))?
            .to_string()
            .as_str()
        {
            "mutex_type" => {
                let buffer = meta.value().map_err(|_| {
                    Error::custom(
                        "mutex_type must be followed by `= [type]`. e.g. mutex_type = NoopRawMutex".to_string(),
                    )
                })?;
                self.mutex_type = Some(buffer.parse()?);
            }
            "packet_type" => {
                let buffer = meta.value().map_err(|_| {
                    Error::custom(
                        "packet_type must be followed by `= [type]`. e.g. packet_type = MyPool".to_string(),
                    )
                })?;
                self.packet_type = Some(buffer.parse()?);
            }
            "attribute_table_size" => {
                let buffer = meta.value().map_err(|_| {
                    Error::custom(
                        "attribute_table_size must be followed by `= [size]`. e.g. attribute_table_size = 32"
                            .to_string(),
                    )
                })?;
                self.attribute_table_size = Some(buffer.parse()?);
            }
            "connections_max" => {
                let buffer = meta.value().map_err(|_| {
                    Error::custom("connections_max must be followed by `= [size]`. e.g. connections_max = 1".to_string())
                })?;
                self.connections_max = Some(buffer.parse()?);
            }
            other => return Err(meta.error(format!(
                "Unsupported server property: '{other}'.\nSupported properties are: mutex_type, packet_type, attribute_table_size, connections_max"
            ))),
        }
        Ok(())
    }
}

pub(crate) struct ServerBuilder {
    properties: syn::ItemStruct,
    arguments: ServerArgs,
}

impl ServerBuilder {
    pub fn new(properties: syn::ItemStruct, arguments: ServerArgs) -> Self {
        Self { properties, arguments }
    }

    /// Construct the macro blueprint for the server struct.
    ///
    /// NeoTime patch: the attribute table is built in a constant and kept in a `static`, so it stays
    /// in flash; only its stored values and the GAP service's borrowed ones are in RAM. See
    /// third_party/trouble-host/NEOTIME.md.
    pub fn build(self) -> TokenStream2 {
        let name = &self.properties.ident;
        let visibility = &self.properties.vis;

        let mutex_type = self.arguments.mutex_type.unwrap_or(syn::Type::Verbatim(quote!(
            trouble_host::__export::embassy_sync::blocking_mutex::raw::NoopRawMutex
        )));

        let packet_type = self
            .arguments
            .packet_type
            .unwrap_or(syn::Type::Verbatim(quote!(trouble_host::prelude::DefaultPacketPool)));

        let layout_name = format_ident!("__{}Layout", name);
        let layout = format_ident!("__{}_LAYOUT", name.to_string().to_uppercase());
        let attributes = format_ident!("__{}_ATTRIBUTES", name.to_string().to_uppercase());
        let attribute_count = format_ident!("__{}_ATTRIBUTE_COUNT", name.to_string().to_uppercase());
        let stored_values = format_ident!("__{}_STORED_VALUES", name.to_string().to_uppercase());
        let stored_bytes = format_ident!("__{}_STORED_BYTES", name.to_string().to_uppercase());

        let mut code_service_definition = TokenStream2::new();
        let mut code_layout_fields = TokenStream2::new();
        let mut code_layout_build = TokenStream2::new();
        let mut code_layout_populate = TokenStream2::new();
        let mut code_service_constants = TokenStream2::new();
        let mut code_server_populate = TokenStream2::new();
        let mut code_server_write = TokenStream2::new();
        let mut code_attribute_summation = TokenStream2::new();
        let mut code_cccd_summation = TokenStream2::new();
        for service in &self.properties.fields {
            let vis = &service.vis;
            let service_span = service.span();
            let service_name = service.ident.as_ref().expect("All fields should have names");
            let service_type = &service.ty;

            let cfg_attrs: Vec<_> = service
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("cfg"))
                .collect();

            code_service_definition.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                #vis #service_name: #service_type,
            });

            code_layout_fields.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                #service_name: #service_type,
            });

            code_layout_build.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                let #service_name = #service_type::build(&mut builder);
            });

            code_layout_populate.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                #service_name,
            });

            // Each service gets its own constant, so using one never copies the whole layout out of flash.
            let service_constant = format_ident!(
                "__{}_{}",
                name.to_string().to_uppercase(),
                service_name.to_string().to_uppercase()
            );
            code_service_constants.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                const #service_constant: #service_type = #layout.#service_name;
            });

            code_server_populate.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                #service_name: #service_constant,
            });

            code_server_write.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)*
                core::ptr::addr_of_mut!((*this).#service_name).write(#service_constant);
            });

            code_attribute_summation.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)* {
                    attr_sum += #service_type::ATTRIBUTE_COUNT;
                }

            });

            code_cccd_summation.extend(quote_spanned! {service_span=>
                #(#cfg_attrs)* {
                   cccd_sum += #service_type::CCCD_COUNT;
                }
            })
        }

        let attr_sum = quote!({let mut attr_sum = trouble_host::gap::GAP_SERVICE_ATTRIBUTE_COUNT;
            #code_attribute_summation
            attr_sum
        });

        let attribute_table_size = if let Some(value) = self.arguments.attribute_table_size {
            value
        } else {
            parse_quote!(#attr_sum)
        };

        let connections_max = if let Some(value) = self.arguments.connections_max {
            value
        } else {
            parse_quote!(1)
        };

        quote! {
            const _ATTRIBUTE_TABLE_SIZE: usize = const {
                let size = #attribute_table_size;
                core::assert!(size >= #attr_sum, "Specified attribute table size is insufficient. Please increase attribute_table_size or remove some services.");
                size
            };
            const _: () = const {
                let exact = {4 + 6 * ({
                    let mut cccd_sum = 0;
                    #code_cccd_summation
                    cccd_sum
                }
                )};
                let configured = trouble_host::config::CLIENT_ATT_TABLE_SIZE;
                core::assert!(
                    configured >= exact,
                    "Configured client ATT table size is insufficient for this server's CCCD attributes. Please increase the client-att-table-size-* feature."
                );
            };
            const _CONNECTIONS_MAX: usize = #connections_max;

            /// The number of attributes the server's table holds.
            const #attribute_count: usize = #attr_sum;

            /// The server's attribute table and services, worked out when the firmware is compiled.
            struct #layout_name {
                attributes: [trouble_host::attribute::Attribute; #attribute_count],
                stored_bytes: usize,
                #code_layout_fields
            }

            const #layout: #layout_name = {
                let mut builder = trouble_host::attribute::AttributeTableBuilder::<#attribute_count>::new();
                trouble_host::gap::add_gap_attributes(&mut builder);
                #code_layout_build
                #layout_name {
                    stored_bytes: builder.stored_bytes(),
                    attributes: builder.finish(),
                    #code_layout_populate
                }
            };

            #code_service_constants

            /// The server's attribute table, in flash.
            static #attributes: [trouble_host::attribute::Attribute; #attribute_count] = #layout.attributes;

            /// The bytes of RAM the server's stored values need.
            const #stored_bytes: usize = #layout.stored_bytes;

            /// The RAM the server's stored values are kept in.
            static #stored_values: trouble_host::__export::static_cell::ConstStaticCell<[u8; #stored_bytes]> =
                trouble_host::__export::static_cell::ConstStaticCell::new([0; #stored_bytes]);

            #visibility struct #name<'values>
            {
                pub server: trouble_host::prelude::AttributeServer<'values, #mutex_type, #packet_type, _ATTRIBUTE_TABLE_SIZE, _CONNECTIONS_MAX>,
                #code_service_definition
            }

            impl<'values> #name<'values>
            {
                /// The attribute table over the flash attributes, its stored values and the GAP service's borrowed ones.
                ///
                /// The stored values' RAM is a static, so a server can be built only once.
                fn __attribute_table(
                    gap: trouble_host::gap::GapConfig<'values>,
                ) -> Result<trouble_host::attribute::AttributeTable<'values, #mutex_type, _ATTRIBUTE_TABLE_SIZE>, &'static str> {
                    let borrowed = gap.borrowed_values()?;
                    Ok(trouble_host::attribute::AttributeTable::new(&#attributes, #stored_values.take(), borrowed))
                }

                /// Create a new Gatt Server instance.
                ///
                /// This function will add a Generic GAP Service with the given name.
                /// The maximum length which the name can be is 22 bytes (limited by the size of the advertising packet).
                /// If a name longer than this is passed, Err() is returned.
                #visibility fn new_default(name: &'values str) -> Result<Self, &'static str> {
                    Self::new_with_config(trouble_host::gap::GapConfig::default(name))
                }

                /// Create a new Gatt Server instance.
                ///
                /// This function will add a GAP Service.
                /// The maximum length which the device name can be is 22 bytes (limited by the size of the advertising packet).
                /// If a name longer than this is passed, Err() is returned.
                #visibility fn new_with_config(gap: trouble_host::gap::GapConfig<'values>) -> Result<Self, &'static str> {
                    let table = Self::__attribute_table(gap)?;
                    Ok(Self {
                        server: trouble_host::prelude::AttributeServer::new(table),
                        #code_server_populate
                    })
                }

                /// NeoTime patch: builds the server with a GAP service straight into `slot`, never copying it.
                #visibility fn init_in_place<'slot>(
                    slot: &'slot mut core::mem::MaybeUninit<Self>,
                    gap: trouble_host::gap::GapConfig<'values>,
                ) -> Result<&'slot mut Self, &'static str> {
                    let table = Self::__attribute_table(gap)?;
                    let this = slot.as_mut_ptr();
                    unsafe {
                        trouble_host::prelude::AttributeServer::init_in_place(core::ptr::addr_of_mut!((*this).server), table);
                        #code_server_write
                    }
                    Ok(unsafe { slot.assume_init_mut() })
                }

                #visibility fn get<T: trouble_host::attribute::AttributeHandle<Value = V>, V: FromGatt>(&self, attribute_handle: &T) -> Result<T::Value, trouble_host::Error> {
                    self.server.table().get(attribute_handle)
                }

                #visibility fn set<T: trouble_host::attribute::AttributeHandle>(&self, attribute_handle: &T, input: &T::Value) -> Result<(), trouble_host::Error> {
                    self.server.table().set(attribute_handle, input)
                }

                #visibility fn get_client_att_table(&self, connection: &trouble_host::connection::Connection<'_, #packet_type>) -> Option<trouble_host::prelude::ClientAttTable> {
                    self.server.get_client_att_table(connection)
                }

                #visibility fn set_client_att_table(&self, connection: &trouble_host::connection::Connection<'_, #packet_type>, table: &trouble_host::prelude::ClientAttTableView<'_>) {
                    self.server.set_client_att_table(connection, table);
                }
            }

            impl<'values> core::ops::Deref for #name<'values>
            {
                type Target = trouble_host::prelude::AttributeServer<'values, #mutex_type, #packet_type, _ATTRIBUTE_TABLE_SIZE, _CONNECTIONS_MAX>;

                fn deref(&self) -> &Self::Target {
                    &self.server
                }
            }

        }
    }
}
