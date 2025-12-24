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
}

impl Parse for HubMethodAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut param_docs = HashMap::new();

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
            for meta in metas {
                match meta {
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
                    _ => {}
                }
            }
        }

        Ok(HubMethodAttrs { name, param_docs })
    }
}

/// Parsed attributes for #[hub_methods]
pub struct HubMethodsAttrs {
    pub namespace: String,
    pub version: String,
    pub description: Option<String>,
    pub crate_path: String,
    /// If true, generate resolve_handle that delegates to self.resolve_handle_impl()
    pub resolve_handle: bool,
}

impl Parse for HubMethodsAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut namespace = String::new();
        let mut version = "1.0.0".to_string();
        let mut description = None;
        let mut crate_path = "crate".to_string();
        let mut resolve_handle = false;

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
                                description = Some(s.value());
                            } else if path.is_ident("crate_path") {
                                crate_path = s.value();
                            }
                        }
                    }
                    Meta::Path(path) => {
                        // Handle bare flags like `resolve_handle`
                        if path.is_ident("resolve_handle") {
                            resolve_handle = true;
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

        Ok(HubMethodsAttrs { namespace, version, description, crate_path, resolve_handle })
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

        // Get param docs from hub_method attrs
        let param_docs = hub_method_attrs
            .map(|a| &a.param_docs)
            .cloned()
            .unwrap_or_default();

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

        Ok(MethodInfo {
            fn_name,
            method_name,
            description,
            params,
            return_type,
            stream_item_type,
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
