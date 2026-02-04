//! HandleEnum derive macro
//!
//! Generates type-safe handle creation and parsing for plugin handle enums.
//!
//! # Example
//!
//! ```ignore
//! #[derive(HandleEnum)]
//! #[handle(plugin_id = "CLAUDECODE_PLUGIN_ID", version = "1.0.0")]
//! pub enum ClaudeCodeHandle {
//!     #[handle(method = "chat_event")]
//!     ChatEvent { event_id: String },
//!
//!     #[handle(method = "message")]
//!     Message { message_id: String, role: String },
//! }
//! ```
//!
//! Generates:
//! - `to_handle(&self) -> Handle` - converts enum variant to Handle
//! - `impl TryFrom<&Handle>` - parses Handle back to enum variant
//! - `impl From<EnumName> for Handle` - convenience conversion

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, punctuated::Punctuated, DeriveInput, Expr, ExprLit, Fields, Lit, Meta,
    MetaList, MetaNameValue, Token,
};

/// Entry point for the derive macro
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Parsed enum-level attributes: #[handle(plugin_id = "...", version = "...")]
struct HandleEnumAttrs {
    /// The constant name holding the plugin UUID (e.g., "CLAUDECODE_PLUGIN_ID")
    plugin_id: String,
    /// Semantic version for handles (e.g., "1.0.0")
    version: String,
    /// Base crate path for imports (default: "plexus_core")
    crate_path: String,
}

impl Parse for HandleEnumAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut plugin_id = None;
        let mut version = None;
        let mut crate_path = "plexus_core".to_string();

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
            for meta in metas {
                if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(s), ..
                    }) = value
                    {
                        if path.is_ident("plugin_id") {
                            plugin_id = Some(s.value());
                        } else if path.is_ident("version") {
                            version = Some(s.value());
                        } else if path.is_ident("crate_path") {
                            crate_path = s.value();
                        }
                    }
                }
            }
        }

        Ok(HandleEnumAttrs {
            plugin_id: plugin_id.ok_or_else(|| {
                syn::Error::new(input.span(), "HandleEnum requires plugin_id = \"...\" attribute")
            })?,
            version: version.ok_or_else(|| {
                syn::Error::new(input.span(), "HandleEnum requires version = \"...\" attribute")
            })?,
            crate_path,
        })
    }
}

/// Parsed variant-level attributes: #[handle(method = "...", table = "...", key = "...", key_field = "...", strip_prefix = "...")]
struct HandleVariantAttrs {
    /// The handle.method value (required)
    method: String,
    /// SQLite table name (optional, for resolution)
    table: Option<String>,
    /// Primary key column (optional, for resolution)
    key: Option<String>,
    /// Which field contains the key value (optional, defaults to first field)
    key_field: Option<String>,
    /// Prefix to strip from key before querying (optional)
    strip_prefix: Option<String>,
}

fn parse_variant_attrs(attrs: &[syn::Attribute]) -> syn::Result<Option<HandleVariantAttrs>> {
    for attr in attrs {
        if attr.path().is_ident("handle") {
            if let Meta::List(MetaList { tokens, .. }) = &attr.meta {
                let mut method = None;
                let mut table = None;
                let mut key = None;
                let mut key_field = None;
                let mut strip_prefix = None;

                let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
                let nested = syn::parse::Parser::parse2(parser, tokens.clone())?;

                for meta in nested {
                    if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            if path.is_ident("method") {
                                method = Some(s.value());
                            } else if path.is_ident("table") {
                                table = Some(s.value());
                            } else if path.is_ident("key") {
                                key = Some(s.value());
                            } else if path.is_ident("key_field") {
                                key_field = Some(s.value());
                            } else if path.is_ident("strip_prefix") {
                                strip_prefix = Some(s.value());
                            }
                        }
                    }
                }

                if let Some(method) = method {
                    return Ok(Some(HandleVariantAttrs { method, table, key, key_field, strip_prefix }));
                }
            }
        }
    }
    Ok(None)
}

/// Information about a variant's named field
struct FieldInfo {
    name: syn::Ident,
    name_str: String,
}

/// Resolution configuration for a variant
struct ResolutionInfo {
    /// SQLite table name
    table: String,
    /// Primary key column name
    key: String,
    /// Which field contains the key value (index into fields)
    key_field_index: usize,
    /// Prefix to strip from key before querying
    strip_prefix: Option<String>,
}

/// Information extracted from an enum variant
struct VariantInfo {
    name: syn::Ident,
    method: String,
    fields: Vec<FieldInfo>,
    /// Resolution configuration (if table and key are specified)
    resolution: Option<ResolutionInfo>,
}

fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let enum_name = &input.ident;

    // Extract enum-level #[handle(...)] attributes
    let enum_attrs = extract_enum_attrs(&input)?;
    let crate_path: syn::Path = syn::parse_str(&enum_attrs.crate_path)?;

    // Parse the plugin_id as an identifier (constant name)
    let plugin_id_ident: syn::Path = syn::parse_str(&enum_attrs.plugin_id).map_err(|e| {
        syn::Error::new_spanned(
            &input,
            format!("Invalid plugin_id constant name '{}': {}", enum_attrs.plugin_id, e),
        )
    })?;

    let version = &enum_attrs.version;

    // Extract variant information
    let variants = extract_variants(&input)?;

    if variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input,
            "HandleEnum requires at least one variant with #[handle(method = \"...\")]",
        ));
    }

    // Generate to_handle() match arms
    let to_handle_arms = generate_to_handle_arms(&variants, &plugin_id_ident, version);

    // Generate TryFrom match arms
    let try_from_arms = generate_try_from_arms(&variants, &crate_path);

    // Generate resolution_params() match arms
    let resolution_arms = generate_resolution_arms(&variants, &crate_path);

    Ok(quote! {
        impl #enum_name {
            /// Convert this handle variant to a generic Handle
            pub fn to_handle(&self) -> #crate_path::Handle {
                match self {
                    #(#to_handle_arms)*
                }
            }

            /// Get resolution parameters for database lookup
            ///
            /// Returns `Some(params)` for variants with `#[handle(table = "...", key = "...")]`,
            /// `None` for variants without resolution configuration.
            pub fn resolution_params(&self) -> Option<#crate_path::HandleResolutionParams> {
                match self {
                    #(#resolution_arms)*
                }
            }
        }

        impl From<#enum_name> for #crate_path::Handle {
            fn from(h: #enum_name) -> #crate_path::Handle {
                h.to_handle()
            }
        }

        impl TryFrom<&#crate_path::Handle> for #enum_name {
            type Error = #crate_path::HandleParseError;

            fn try_from(handle: &#crate_path::Handle) -> Result<Self, Self::Error> {
                // Verify plugin ownership
                if handle.plugin_id != #plugin_id_ident {
                    return Err(#crate_path::HandleParseError::WrongPlugin {
                        expected: #plugin_id_ident,
                        got: handle.plugin_id,
                    });
                }

                match handle.method.as_str() {
                    #(#try_from_arms)*
                    _ => Err(#crate_path::HandleParseError::UnknownMethod(handle.method.clone()))
                }
            }
        }
    })
}

fn extract_enum_attrs(input: &DeriveInput) -> syn::Result<HandleEnumAttrs> {
    for attr in &input.attrs {
        if attr.path().is_ident("handle") {
            if let Meta::List(MetaList { tokens, .. }) = &attr.meta {
                return syn::parse2(tokens.clone());
            }
        }
    }
    Err(syn::Error::new_spanned(
        input,
        "HandleEnum requires #[handle(plugin_id = \"...\", version = \"...\")] attribute",
    ))
}

fn extract_variants(input: &DeriveInput) -> syn::Result<Vec<VariantInfo>> {
    let data_enum = match &input.data {
        syn::Data::Enum(data) => data,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "HandleEnum can only be derived for enums",
            ))
        }
    };

    let mut variants = Vec::new();

    for variant in &data_enum.variants {
        if let Some(attrs) = parse_variant_attrs(&variant.attrs)? {
            let fields: Vec<FieldInfo> = match &variant.fields {
                Fields::Named(named) => named
                    .named
                    .iter()
                    .map(|f| {
                        let name = f.ident.clone().unwrap();
                        let name_str = name.to_string();
                        FieldInfo { name, name_str }
                    })
                    .collect(),
                Fields::Unit => Vec::new(),
                Fields::Unnamed(_) => {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "HandleEnum variants must use named fields (e.g., `Variant { field: Type }`)",
                    ))
                }
            };

            // Build resolution info if table and key are specified
            let resolution = if let (Some(table), Some(key)) = (attrs.table, attrs.key) {
                // Find the key field index
                let key_field_name = attrs.key_field.unwrap_or_else(|| {
                    // Default to first field
                    fields.first().map(|f| f.name_str.clone()).unwrap_or_default()
                });
                let key_field_index = fields
                    .iter()
                    .position(|f| f.name_str == key_field_name)
                    .unwrap_or(0);

                Some(ResolutionInfo {
                    table,
                    key,
                    key_field_index,
                    strip_prefix: attrs.strip_prefix,
                })
            } else {
                None
            };

            variants.push(VariantInfo {
                name: variant.ident.clone(),
                method: attrs.method,
                fields,
                resolution,
            });
        }
    }

    Ok(variants)
}

fn generate_to_handle_arms(
    variants: &[VariantInfo],
    plugin_id_ident: &syn::Path,
    version: &str,
) -> Vec<TokenStream2> {
    variants
        .iter()
        .map(|v| {
            let variant_name = &v.name;
            let method = &v.method;

            if v.fields.is_empty() {
                // Unit-like variant with #[handle]
                quote! {
                    Self::#variant_name => {
                        plexus_core::Handle::new(#plugin_id_ident, #version, #method)
                    }
                }
            } else {
                // Named fields - extract them
                let field_names: Vec<_> = v.fields.iter().map(|f| &f.name).collect();
                let field_clones: Vec<_> = v
                    .fields
                    .iter()
                    .map(|f| {
                        let name = &f.name;
                        quote! { #name.clone() }
                    })
                    .collect();

                quote! {
                    Self::#variant_name { #(#field_names),* } => {
                        plexus_core::Handle::new(#plugin_id_ident, #version, #method)
                            .with_meta(vec![#(#field_clones),*])
                    }
                }
            }
        })
        .collect()
}

fn generate_try_from_arms(variants: &[VariantInfo], crate_path: &syn::Path) -> Vec<TokenStream2> {
    variants
        .iter()
        .map(|v| {
            let variant_name = &v.name;
            let method = &v.method;

            if v.fields.is_empty() {
                // Unit-like variant
                quote! {
                    #method => Ok(Self::#variant_name),
                }
            } else {
                // Named fields - extract from meta
                let field_extractions: Vec<_> = v
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(idx, f)| {
                        let name = &f.name;
                        let name_str = &f.name_str;
                        quote! {
                            let #name = handle.meta.get(#idx)
                                .ok_or(#crate_path::HandleParseError::MissingMeta {
                                    index: #idx,
                                    field: #name_str,
                                })?
                                .clone();
                        }
                    })
                    .collect();

                let field_names: Vec<_> = v.fields.iter().map(|f| &f.name).collect();

                quote! {
                    #method => {
                        #(#field_extractions)*
                        Ok(Self::#variant_name { #(#field_names),* })
                    }
                }
            }
        })
        .collect()
}

fn generate_resolution_arms(variants: &[VariantInfo], crate_path: &syn::Path) -> Vec<TokenStream2> {
    variants
        .iter()
        .map(|v| {
            let variant_name = &v.name;

            match &v.resolution {
                Some(res) => {
                    let table = &res.table;
                    let key = &res.key;
                    let key_field_index = res.key_field_index;
                    let key_field_name = &v.fields[key_field_index].name;

                    // Build context from non-key fields
                    let context_items: Vec<_> = v
                        .fields
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| *idx != key_field_index)
                        .map(|(_, f)| {
                            let name = &f.name;
                            let name_str = &f.name_str;
                            quote! {
                                (#name_str.to_string(), #name.clone())
                            }
                        })
                        .collect();

                    // Handle strip_prefix
                    let key_value_expr = if let Some(prefix) = &res.strip_prefix {
                        quote! {
                            #key_field_name.strip_prefix(#prefix).unwrap_or(&#key_field_name).to_string()
                        }
                    } else {
                        quote! {
                            #key_field_name.clone()
                        }
                    };

                    if v.fields.is_empty() {
                        quote! {
                            Self::#variant_name => None,
                        }
                    } else {
                        let field_names: Vec<_> = v.fields.iter().map(|f| &f.name).collect();
                        quote! {
                            Self::#variant_name { #(#field_names),* } => Some(#crate_path::HandleResolutionParams {
                                table: #table,
                                key_column: #key,
                                key_value: #key_value_expr,
                                context: vec![#(#context_items),*],
                            }),
                        }
                    }
                }
                None => {
                    // No resolution configured - use `..` to ignore all fields
                    if v.fields.is_empty() {
                        quote! {
                            Self::#variant_name => None,
                        }
                    } else {
                        quote! {
                            Self::#variant_name { .. } => None,
                        }
                    }
                }
            }
        })
        .collect()
}
