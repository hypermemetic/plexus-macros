//! `#[derive(PlexusRequest)]` proc macro implementation.
//!
//! Generates:
//! - `impl PlexusRequest for T { fn extract(...) }` — typed extraction from RawRequestContext
//! - `impl PlexusRequest for T { fn request_schema() }` — JSON schema with x-plexus-source metadata
//! - `impl schemars::JsonSchema for T` — real schemars JsonSchema impl via plexus_core::__schemars
//!
//! The `impl schemars::JsonSchema` is generated using `::plexus_core::__schemars::JsonSchema` so
//! that callers do not need to have schemars in their own dependency tree.
//!
//! The plexus-schemars-compat shim exists for test crates that write
//! `#[derive(PlexusRequest, schemars::JsonSchema)]` — the no-op `JsonSchema` derive from the shim
//! prevents a duplicate `impl JsonSchema` error since this macro already generates one.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data, DeriveInput, Field, Fields, Ident, LitStr, Type,
};

/// Parsed source annotation for a single field.
#[derive(Debug, Clone)]
enum FieldSource {
    Cookie(String),
    Header(String),
    Query(String),
    Peer,
    AuthContext,
    Default,
}

/// Parsed info for a single field.
struct FieldInfo {
    ident: Ident,
    ty: Type,
    source: FieldSource,
    is_option: bool,
}

pub fn derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input);
    match derive_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => named.named.iter().cloned().collect::<Vec<_>>(),
            _ => {
                return Err(syn::Error::new_spanned(
                    struct_name,
                    "PlexusRequest can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                struct_name,
                "PlexusRequest can only be derived for structs",
            ))
        }
    };

    let field_infos: Vec<FieldInfo> = fields
        .iter()
        .map(parse_field)
        .collect::<syn::Result<Vec<_>>>()?;

    let extract_impl = generate_extract_impl(struct_name, &field_infos, &input);

    Ok(extract_impl)
}

fn parse_field(field: &Field) -> syn::Result<FieldInfo> {
    let ident = field.ident.clone().ok_or_else(|| {
        syn::Error::new_spanned(field, "unnamed fields are not supported")
    })?;

    let mut source = FieldSource::Default;

    for attr in &field.attrs {
        if attr.path().is_ident("from_cookie") {
            let key: LitStr = attr.parse_args()?;
            source = FieldSource::Cookie(key.value());
        } else if attr.path().is_ident("from_header") {
            let key: LitStr = attr.parse_args()?;
            source = FieldSource::Header(key.value());
        } else if attr.path().is_ident("from_query") {
            let key: LitStr = attr.parse_args()?;
            source = FieldSource::Query(key.value());
        } else if attr.path().is_ident("from_peer") {
            source = FieldSource::Peer;
        } else if attr.path().is_ident("from_auth_context") {
            source = FieldSource::AuthContext;
        }
    }

    let is_option = is_option_type(&field.ty);

    Ok(FieldInfo {
        ident,
        ty: field.ty.clone(),
        source,
        is_option,
    })
}

/// Returns true if the type is `Option<T>` for any T.
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Option";
        }
    }
    false
}

/// Generate the `impl PlexusRequest` block with `extract()` and `request_schema()`,
/// and an `impl schemars::JsonSchema` block via `::plexus_core::__schemars`.
fn generate_extract_impl(struct_name: &Ident, fields: &[FieldInfo], input: &DeriveInput) -> TokenStream2 {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let field_extractions: Vec<TokenStream2> = fields
        .iter()
        .map(generate_field_extraction)
        .collect();

    let field_names: Vec<&Ident> = fields.iter().map(|f| &f.ident).collect();

    // Generate the request_schema() body - builds JSON schema inline with x-plexus-source
    let schema_value = generate_schema_value(fields);

    // For JsonSchema impl, we reuse the same inline schema construction.
    // The Schema type in schemars 1.1 implements TryFrom<serde_json::Value>.
    let struct_name_str = struct_name.to_string();

    quote! {
        #[automatically_derived]
        impl #impl_generics ::plexus_core::request::PlexusRequest for #struct_name #ty_generics #where_clause {
            fn extract(
                ctx: &::plexus_core::request::RawRequestContext
            ) -> ::core::result::Result<Self, ::plexus_core::plexus::PlexusError> {
                #(#field_extractions)*
                ::core::result::Result::Ok(Self {
                    #(#field_names),*
                })
            }

            fn request_schema() -> ::core::option::Option<::serde_json::Value> {
                ::core::option::Option::Some(#schema_value)
            }
        }

        #[automatically_derived]
        impl #impl_generics ::plexus_core::__schemars::JsonSchema for #struct_name #ty_generics #where_clause {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(#struct_name_str)
            }

            fn json_schema(
                _gen: &mut ::plexus_core::__schemars::SchemaGenerator,
            ) -> ::plexus_core::__schemars::Schema {
                // schema_value() returns a serde_json::Value::Object.
                // Schema implements From<Map<String, Value>> and TryFrom<Value>.
                let value: ::serde_json::Value = #schema_value;
                match value {
                    ::serde_json::Value::Object(map) => {
                        <::plexus_core::__schemars::Schema as ::core::convert::From<
                            ::serde_json::Map<::std::string::String, ::serde_json::Value>,
                        >>::from(map)
                    }
                    _ => <::plexus_core::__schemars::Schema as ::core::convert::From<
                        ::serde_json::Map<::std::string::String, ::serde_json::Value>,
                    >>::from(::serde_json::Map::new()),
                }
            }
        }
    }
}

/// Generate extraction code for a single field.
fn generate_field_extraction(field: &FieldInfo) -> TokenStream2 {
    let name = &field.ident;

    match &field.source {
        FieldSource::Cookie(key) => {
            let key_lit = key.as_str();
            if field.is_option {
                // Optional cookie: absent → None, present → Some(parse_result.ok())
                quote! {
                    let #name = {
                        let _cookie_hdr = ctx.headers.get("cookie")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        ::plexus_core::request::parse_cookie(_cookie_hdr, #key_lit)
                            .and_then(|v| v.parse().ok())
                    };
                }
            } else {
                // Required cookie: absent or parse failure → Err(Unauthenticated)
                quote! {
                    let #name = {
                        let _cookie_hdr = ctx.headers.get("cookie")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        let _raw = ::plexus_core::request::parse_cookie(_cookie_hdr, #key_lit)
                            .ok_or_else(|| ::plexus_core::plexus::PlexusError::Unauthenticated(
                                ::std::string::String::from("Authentication required")
                            ))?;
                        // For String fields: return raw value; for other types: parse
                        _raw.parse::<_>().map_err(|_| {
                            ::plexus_core::plexus::PlexusError::Unauthenticated(
                                ::std::format!("Cookie '{}' could not be parsed", #key_lit)
                            )
                        })?
                    };
                }
            }
        }

        FieldSource::Header(key) => {
            let key_lit = key.as_str();
            if field.is_option {
                quote! {
                    let #name = ctx.headers.get(#key_lit)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok());
                }
            } else {
                quote! {
                    let #name = ctx.headers.get(#key_lit)
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(|| ::plexus_core::plexus::PlexusError::Unauthenticated(
                            ::std::format!(
                                "Required header '{}' not present",
                                #key_lit
                            )
                        ))
                        .and_then(|s| s.parse::<_>().map_err(|_| {
                            ::plexus_core::plexus::PlexusError::Unauthenticated(
                                ::std::format!("Header '{}' could not be parsed", #key_lit)
                            )
                        }))?;
                }
            }
        }

        FieldSource::Query(key) => {
            let key_lit = key.as_str();
            if field.is_option {
                quote! {
                    let #name = form_urlencoded::parse(ctx.uri.query().unwrap_or("").as_bytes())
                        .find(|(k, _)| k == #key_lit)
                        .and_then(|(_, v)| v.parse().ok());
                }
            } else {
                quote! {
                    let #name = {
                        let _raw = form_urlencoded::parse(ctx.uri.query().unwrap_or("").as_bytes())
                            .find(|(k, _)| k == #key_lit)
                            .map(|(_, v)| v.into_owned())
                            .ok_or_else(|| ::plexus_core::plexus::PlexusError::Unauthenticated(
                                ::std::format!(
                                    "Required query parameter '{}' not present",
                                    #key_lit
                                )
                            ))?;
                        _raw.parse::<_>().map_err(|_| {
                            ::plexus_core::plexus::PlexusError::Unauthenticated(
                                ::std::format!("Query param '{}' could not be parsed", #key_lit)
                            )
                        })?
                    };
                }
            }
        }

        FieldSource::Peer => {
            if field.is_option {
                quote! { let #name = ctx.peer; }
            } else {
                quote! {
                    let #name = ctx.peer.ok_or_else(|| {
                        ::plexus_core::plexus::PlexusError::Unauthenticated(
                            ::std::string::String::from("Peer address not available")
                        )
                    })?;
                }
            }
        }

        FieldSource::AuthContext => {
            if field.is_option {
                quote! { let #name = ctx.auth.clone(); }
            } else {
                quote! {
                    let #name = ctx.auth.clone().ok_or_else(|| {
                        ::plexus_core::plexus::PlexusError::Unauthenticated(
                            ::std::string::String::from("Authentication required")
                        )
                    })?;
                }
            }
        }

        FieldSource::Default => {
            let ty = &field.ty;
            // Fields with no explicit source annotation must implement PlexusRequestField.
            // The trait's extract_from_raw handles all validation internally.
            quote! {
                let #name = <#ty as ::plexus_core::plexus::PlexusRequestField>::extract_from_raw(ctx)?;
            }
        }
    }
}

/// Generate a `serde_json::Value` expression that builds the schema inline.
///
/// This is called inside the generated `request_schema()` fn, and returns a
/// `serde_json::json!({...})` literal with `x-plexus-source` on each field.
fn generate_schema_value(fields: &[FieldInfo]) -> TokenStream2 {
    // Build property entries as token stream pairs
    let property_entries: Vec<TokenStream2> = fields.iter().map(|f| {
        let fname_str = f.ident.to_string();

        let source_json = match &f.source {
            FieldSource::Cookie(key) => quote! {
                ::serde_json::json!({
                    "type": "string",
                    "x-plexus-source": { "from": "cookie", "key": #key }
                })
            },
            FieldSource::Header(key) => {
                if f.is_option {
                    quote! {
                        ::serde_json::json!({
                            "anyOf": [{"type": "string"}, {"type": "null"}],
                            "x-plexus-source": { "from": "header", "key": #key }
                        })
                    }
                } else {
                    quote! {
                        ::serde_json::json!({
                            "type": "string",
                            "x-plexus-source": { "from": "header", "key": #key }
                        })
                    }
                }
            },
            FieldSource::Query(key) => {
                if f.is_option {
                    quote! {
                        ::serde_json::json!({
                            "anyOf": [{"type": "string"}, {"type": "null"}],
                            "x-plexus-source": { "from": "query", "key": #key }
                        })
                    }
                } else {
                    quote! {
                        ::serde_json::json!({
                            "type": "string",
                            "x-plexus-source": { "from": "query", "key": #key }
                        })
                    }
                }
            },
            FieldSource::Peer | FieldSource::AuthContext => quote! {
                ::serde_json::json!({
                    "x-plexus-source": { "from": "derived" }
                })
            },
            FieldSource::Default => quote! {
                ::serde_json::json!({
                    "x-plexus-source": { "from": "derived" }
                })
            },
        };

        quote! {
            __props.insert(
                ::std::string::String::from(#fname_str),
                #source_json,
            );
        }
    }).collect();

    // Build required array entries (non-optional fields)
    let required_entries: Vec<TokenStream2> = fields.iter()
        .filter(|f| !f.is_option)
        .map(|f| {
            let fname_str = f.ident.to_string();
            quote! {
                __required.push(::serde_json::Value::String(::std::string::String::from(#fname_str)));
            }
        })
        .collect();

    quote! {
        {
            let mut __props = ::serde_json::Map::new();
            let mut __required: ::std::vec::Vec<::serde_json::Value> = ::std::vec::Vec::new();

            #(#property_entries)*
            #(#required_entries)*

            let mut __obj = ::serde_json::Map::new();
            __obj.insert(
                ::std::string::String::from("type"),
                ::serde_json::Value::String(::std::string::String::from("object")),
            );
            __obj.insert(
                ::std::string::String::from("properties"),
                ::serde_json::Value::Object(__props),
            );
            if !__required.is_empty() {
                __obj.insert(
                    ::std::string::String::from("required"),
                    ::serde_json::Value::Array(__required),
                );
            }
            ::serde_json::Value::Object(__obj)
        }
    }
}
