//! Generate method enum from hub methods

use crate::parse::MethodInfo;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(struct_name: &syn::Ident, methods: &[MethodInfo], crate_path: &syn::Path) -> TokenStream {
    let enum_name = format_ident!("{}Method", struct_name);

    let variants: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            let variant_name = format_ident!("{}", to_pascal_case(&m.method_name));
            let method_name = &m.method_name;
            let doc = &m.description;

            // Always use struct variants for consistent schema generation.
            // This ensures params is always an object with named fields.
            // Use serde(rename) to get the exact method name in the schema.
            match m.params.len() {
                0 => quote! {
                    #[doc = #doc]
                    #[serde(rename = #method_name)]
                    #variant_name
                },
                _ => {
                    // Use struct variant for all cases (including single param)
                    // This produces proper object schema with field names
                    // Add description for each field from param_docs or fallback to name
                    let fields: Vec<TokenStream> = m
                        .params
                        .iter()
                        .map(|p| {
                            let name = &p.name;
                            let ty = &p.ty;
                            let desc = p.description.clone()
                                .unwrap_or_else(|| format!("The {} parameter", name));
                            quote! {
                                #[schemars(description = #desc)]
                                #name: #ty
                            }
                        })
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
    let method_descriptions: Vec<&str> = methods.iter().map(|m| m.description.as_str()).collect();

    // Generate return type schemas for each method
    let return_schema_entries: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            if let Some(item_ty) = &m.stream_item_type {
                quote! { Some(schemars::schema_for!(#item_ty)) }
            } else {
                quote! { None }
            }
        })
        .collect();

    quote! {
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
                // schemars 1.1+ already generates const values for adjacently tagged enums
                // Just return the schema directly
                serde_json::to_value(schemars::schema_for!(#enum_name)).expect("Schema should serialize")
            }
        }

        impl #enum_name {
            /// Get per-method schema info including params and return types
            pub fn method_schemas() -> Vec<#crate_path::plexus::MethodSchema> {
                let method_names: &[&str] = &[#(#method_names),*];
                let descriptions: &[&str] = &[#(#method_descriptions),*];
                let return_schemas: Vec<Option<schemars::Schema>> = vec![#(#return_schema_entries),*];

                // Get the full enum schema and extract each variant
                let full_schema = schemars::schema_for!(#enum_name);
                let schema_value = serde_json::to_value(&full_schema).expect("Schema should serialize");

                // Extract oneOf variants from the schema
                let one_of = schema_value
                    .get("oneOf")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                method_names
                    .iter()
                    .zip(descriptions.iter())
                    .zip(return_schemas.into_iter())
                    .enumerate()
                    .map(|(i, ((name, desc), returns))| {
                        // Get this variant's schema from oneOf, then extract just the "params" portion
                        // The variant looks like: { properties: { method: {...}, params: {...} }, ... }
                        // We want just the params schema
                        let params = one_of.get(i).and_then(|variant| {
                            variant
                                .get("properties")
                                .and_then(|props| props.get("params"))
                                .cloned()
                                .and_then(|p| serde_json::from_value::<schemars::Schema>(p).ok())
                        });

                        let mut schema = #crate_path::plexus::MethodSchema::new(name.to_string(), desc.to_string());
                        if let Some(p) = params {
                            schema = schema.with_params(p);
                        }
                        if let Some(r) = returns {
                            schema = schema.with_returns(r);
                        }
                        schema
                    })
                    .collect()
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
