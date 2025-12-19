//! Generate method enum from hub methods

use crate::parse::MethodInfo;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(struct_name: &syn::Ident, methods: &[MethodInfo], crate_path: &syn::Path) -> TokenStream {
    let enum_name = format_ident!("{}Method", struct_name);
    let schema_name = format_ident!("{}Schema", struct_name);

    // Generate params structs for each method (for proper schema generation)
    let params_structs: Vec<TokenStream> = methods
        .iter()
        .filter(|m| !m.params.is_empty())
        .map(|m| {
            let struct_name = format_ident!("{}Params", to_pascal_case(&m.method_name));
            let fields: Vec<TokenStream> = m
                .params
                .iter()
                .map(|(name, ty)| {
                    let doc = format!("The {} parameter", name);
                    quote! {
                        #[doc = #doc]
                        pub #name: #ty
                    }
                })
                .collect();
            quote! {
                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
                struct #struct_name {
                    #(#fields),*
                }
            }
        })
        .collect();

    let variants: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            let variant_name = format_ident!("{}", to_pascal_case(&m.method_name));
            let method_name = &m.method_name;
            let doc = &m.description;

            // Always use struct variants for consistent schema generation.
            // This ensures params is always an object with named fields.
            // Also explicitly rename to snake_case method name for const discriminator.
            match m.params.len() {
                0 => quote! {
                    #[doc = #doc]
                    #[serde(rename = #method_name)]
                    #variant_name
                },
                _ => {
                    // Use struct variant for all cases (including single param)
                    // This produces proper object schema with field names
                    let fields: Vec<TokenStream> = m
                        .params
                        .iter()
                        .map(|(name, ty)| quote! { #name: #ty })
                        .collect();
                    quote! {
                        #[doc = #doc]
                        #[serde(rename = #method_name)]
                        #variant_name { #(#fields),* }
                    }
                }
            }
        })
        .collect();

    let method_names: Vec<&str> = methods.iter().map(|m| m.method_name.as_str()).collect();

    // Generate method schema entries with return types
    let method_schema_entries: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            let name = &m.method_name;
            let desc = &m.description;

            // Get the stream item type if available
            let returns_schema = if let Some(item_ty) = &m.stream_item_type {
                quote! { Some(schemars::schema_for!(#item_ty)) }
            } else {
                quote! { None }
            };

            // Get params schema - use generated params struct for proper naming
            let params_schema = if m.params.is_empty() {
                quote! { None }
            } else {
                let params_struct_name = format_ident!("{}Params", to_pascal_case(&m.method_name));
                quote! { Some(schemars::schema_for!(#params_struct_name)) }
            };

            quote! {
                MethodSchema {
                    name: #name.to_string(),
                    description: #desc.to_string(),
                    params: #params_schema,
                    returns: #returns_schema,
                }
            }
        })
        .collect();

    quote! {
        // Generated params structs for schema extraction
        #(#params_structs)*

        /// Auto-generated method enum for schema extraction
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(tag = "method", content = "params", rename_all = "snake_case")]
        pub enum #enum_name {
            #(#variants),*
        }

        impl #enum_name {
            pub fn all_method_names() -> &'static [&'static str] {
                &[#(#method_names),*]
            }
        }

        impl #crate_path::plexus::MethodEnumSchema for #enum_name {
            fn method_names() -> &'static [&'static str] {
                &[#(#method_names),*]
            }

            fn schema_with_consts() -> serde_json::Value {
                use schemars::JsonSchema;
                let schema = Self::json_schema(&mut schemars::SchemaGenerator::default());
                let mut value = serde_json::to_value(schema).expect("Schema should serialize");
                let method_names = Self::method_names();

                // Transform the schema to add const values for method discriminators
                if let Some(obj) = value.as_object_mut() {
                    if let Some(one_of) = obj.get_mut("oneOf") {
                        if let Some(variants) = one_of.as_array_mut() {
                            for (i, variant) in variants.iter_mut().enumerate() {
                                if let Some(variant_obj) = variant.as_object_mut() {
                                    if let Some(props) = variant_obj.get_mut("properties") {
                                        if let Some(props_obj) = props.as_object_mut() {
                                            if let Some(method_prop) = props_obj.get_mut("method") {
                                                if let Some(method_obj) = method_prop.as_object_mut() {
                                                    method_obj.remove("type");
                                                    if let Some(name) = method_names.get(i) {
                                                        method_obj.insert(
                                                            "const".to_string(),
                                                            serde_json::Value::String(name.to_string()),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                value
            }
        }

        /// Schema for a single method including params and return type
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct MethodSchema {
            pub name: String,
            pub description: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub params: Option<schemars::Schema>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub returns: Option<schemars::Schema>,
        }

        /// Full schema for this activation including all methods with their types
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #schema_name {
            pub methods: Vec<MethodSchema>,
        }

        impl #schema_name {
            pub fn generate() -> Self {
                Self {
                    methods: vec![#(#method_schema_entries),*],
                }
            }
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
