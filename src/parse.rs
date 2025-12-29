//! Attribute parsing for hub macros

use std::collections::HashMap;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Expr, ExprLit, FnArg, GenericArgument, ImplItemFn, Lit, Meta, MetaList, MetaNameValue, Pat,
    PathArguments, ReturnType, Token, Type,
};

/// Parsed attributes for #[hub_method]
pub struct HubMethodAttrs {
    pub name: Option<String>,
    /// Parameter descriptions: param_name -> description
    pub param_docs: HashMap<String, String>,
    /// If true, this method overrides dispatch (returns PlexusStream directly)
    pub override_call: bool,
}

impl Parse for HubMethodAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut param_docs = HashMap::new();
        let mut override_call = false;

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
            for meta in metas {
                match meta {
                    Meta::Path(path) => {
                        if path.is_ident("override_call") {
                            override_call = true;
                        }
                    }
                    Meta::NameValue(MetaNameValue { path, value, .. }) => {
                        if path.is_ident("name") {
                            if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                                name = Some(s.value());
                            }
                        }
                    }
                    Meta::List(MetaList { path, tokens, .. }) => {
                        if path.is_ident("params") {
                            // Parse params(name = "desc", other = "desc2")
                            let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
                            let nested = syn::parse::Parser::parse2(parser, tokens.clone())?;
                            for meta in nested {
                                if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                                    if let Some(ident) = path.get_ident() {
                                        if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                                            param_docs.insert(ident.to_string(), s.value());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(HubMethodAttrs { name, param_docs, override_call })
    }
}

/// Maximum word count for description
const MAX_DESCRIPTION_WORDS: usize = 15;

/// Parsed attributes for #[hub_methods]
pub struct HubMethodsAttrs {
    pub namespace: String,
    pub version: String,
    /// Short description (max 15 words)
    pub description: Option<String>,
    /// Long description (optional, for detailed documentation)
    pub long_description: Option<String>,
    pub crate_path: String,
    /// If true, generate resolve_handle that delegates to self.resolve_handle_impl()
    pub resolve_handle: bool,
    /// If true, this activation is a hub with children (calls self.plugin_children())
    pub hub: bool,
    /// Stable UUID for this plugin instance (for handle routing)
    /// If not provided, a deterministic UUID is generated from namespace
    pub plugin_id: Option<String>,
}

impl Parse for HubMethodsAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut namespace = String::new();
        let mut version = "1.0.0".to_string();
        let mut description = None;
        let mut long_description = None;
        let mut crate_path = "crate".to_string();
        let mut resolve_handle = false;
        let mut hub = false;
        let mut plugin_id = None;

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;

            for meta in metas {
                match meta {
                    Meta::NameValue(MetaNameValue { path, value, .. }) => {
                        if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                            if path.is_ident("namespace") {
                                namespace = s.value();
                            } else if path.is_ident("version") {
                                version = s.value();
                            } else if path.is_ident("description") {
                                let desc = s.value();
                                // Validate word count
                                let word_count = desc.split_whitespace().count();
                                if word_count > MAX_DESCRIPTION_WORDS {
                                    return Err(syn::Error::new(
                                        s.span(),
                                        format!(
                                            "description must be {} words or fewer (found {} words). \
                                            Use long_description for detailed text.",
                                            MAX_DESCRIPTION_WORDS, word_count
                                        ),
                                    ));
                                }
                                description = Some(desc);
                            } else if path.is_ident("long_description") {
                                long_description = Some(s.value());
                            } else if path.is_ident("crate_path") {
                                crate_path = s.value();
                            } else if path.is_ident("plugin_id") {
                                // Validate UUID format
                                let id_str = s.value();
                                if uuid::Uuid::parse_str(&id_str).is_err() {
                                    return Err(syn::Error::new(
                                        s.span(),
                                        format!("plugin_id must be a valid UUID, got: {}", id_str),
                                    ));
                                }
                                plugin_id = Some(id_str);
                            }
                        }
                    }
                    Meta::Path(path) => {
                        // Handle bare flags like `resolve_handle`, `hub`
                        if path.is_ident("resolve_handle") {
                            resolve_handle = true;
                        } else if path.is_ident("hub") {
                            hub = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        if namespace.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "hub_methods requires namespace = \"...\" attribute",
            ));
        }

        Ok(HubMethodsAttrs { namespace, version, description, long_description, crate_path, resolve_handle, hub, plugin_id })
    }
}

/// Parameter info with optional description
pub struct ParamInfo {
    pub name: syn::Ident,
    pub ty: Type,
    pub description: Option<String>,
}

/// Information extracted from a #[hub_method] function
pub struct MethodInfo {
    pub fn_name: syn::Ident,
    pub method_name: String,
    pub description: String,
    pub params: Vec<ParamInfo>,
    pub return_type: Type,
    pub stream_item_type: Option<Type>,
    /// True if method has #[hub_method(override_call)] - returns PlexusStream directly
    pub is_override: bool,
}

impl MethodInfo {
    /// Extract method info from an impl function with optional hub_method attrs
    pub fn from_fn(method: &ImplItemFn, hub_method_attrs: Option<&HubMethodAttrs>) -> syn::Result<Self> {
        let fn_name = method.sig.ident.clone();
        let method_name = hub_method_attrs
            .and_then(|a| a.name.clone())
            .unwrap_or_else(|| fn_name.to_string());

        // Extract doc attributes and description
        let mut doc_lines = Vec::new();
        for attr in &method.attrs {
            if attr.path().is_ident("doc") {
                if let Meta::NameValue(MetaNameValue { value, .. }) = &attr.meta {
                    if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                        doc_lines.push(s.value().trim().to_string());
                    }
                }
            }
        }
        let description = doc_lines.join(" ");

        // Get param docs and override_call from hub_method attrs
        let param_docs = hub_method_attrs
            .map(|a| &a.param_docs)
            .cloned()
            .unwrap_or_default();
        let is_override = hub_method_attrs
            .map(|a| a.override_call)
            .unwrap_or(false);

        // Extract parameters after &self
        let mut params = Vec::new();
        for arg in &method.sig.inputs {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(ident) = &*pat_type.pat {
                    let name = ident.ident.clone();
                    let name_str = name.to_string();
                    let description = param_docs.get(&name_str).cloned();
                    params.push(ParamInfo {
                        name,
                        ty: (*pat_type.ty).clone(),
                        description,
                    });
                }
            }
        }

        // Extract return type
        let return_type = match &method.sig.output {
            ReturnType::Default => {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "hub_method requires a return type",
                ))
            }
            ReturnType::Type(_, ty) => (**ty).clone(),
        };

        let stream_item_type = extract_stream_item_type(&return_type);

        // Check for conflicting return type without override_call
        let looks_like_passthrough = is_result_plexus_stream(&return_type);
        if looks_like_passthrough && !is_override {
            return Err(syn::Error::new_spanned(
                &method.sig.output,
                format!(
                    "Method `{}` returns Result<PlexusStream, _> which conflicts with \
                    generated wrapping. Add #[hub_macro::hub_method(override_call)] if you \
                    want to handle dispatch manually, or change return type to \
                    `impl Stream<Item = T>` for automatic wrapping.",
                    fn_name
                ),
            ));
        }

        Ok(MethodInfo {
            fn_name,
            method_name,
            description,
            params,
            return_type,
            stream_item_type,
            is_override,
        })
    }
}

/// Extract Item type from `impl Stream<Item = T>`
fn extract_stream_item_type(ty: &Type) -> Option<Type> {
    if let Type::ImplTrait(impl_trait) = ty {
        for bound in &impl_trait.bounds {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                let last_segment = trait_bound.path.segments.last()?;
                if last_segment.ident == "Stream" {
                    if let PathArguments::AngleBracketed(args) = &last_segment.arguments {
                        for arg in &args.args {
                            if let GenericArgument::AssocType(assoc) = arg {
                                if assoc.ident == "Item" {
                                    return Some(assoc.ty.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if return type looks like Result<PlexusStream, _>
/// This detects types that should use override_call instead of automatic wrapping
fn is_result_plexus_stream(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(first_type)) = args.args.first() {
                        // Check if first type arg is PlexusStream
                        if let Type::Path(inner_path) = first_type {
                            if let Some(inner_seg) = inner_path.path.segments.last() {
                                return inner_seg.ident == "PlexusStream";
                            }
                        }
                    }
                }
            }
        }
    }
    false
}
