//! Attribute parsing for hub macros

use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Expr, ExprLit, FnArg, GenericArgument, ImplItemFn, Lit, Meta, MetaNameValue, Pat,
    PathArguments, ReturnType, Token, Type,
};

/// Parsed attributes for #[hub_method]
pub struct HubMethodAttrs {
    pub name: Option<String>,
}

impl Parse for HubMethodAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
            for meta in metas {
                if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                    if path.is_ident("name") {
                        if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                            name = Some(s.value());
                        }
                    }
                }
            }
        }

        Ok(HubMethodAttrs { name })
    }
}

/// Parsed attributes for #[hub_methods]
pub struct HubMethodsAttrs {
    pub namespace: String,
    pub version: String,
    pub description: Option<String>,
    pub crate_path: String,
}

impl Parse for HubMethodsAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut namespace = String::new();
        let mut version = "1.0.0".to_string();
        let mut description = None;
        let mut crate_path = "crate".to_string();

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;

            for meta in metas {
                if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
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
            }
        }

        if namespace.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "hub_methods requires namespace = \"...\" attribute",
            ));
        }

        Ok(HubMethodsAttrs { namespace, version, description, crate_path })
    }
}

/// Information extracted from a #[hub_method] function
pub struct MethodInfo {
    pub fn_name: syn::Ident,
    pub method_name: String,
    pub description: String,
    pub params: Vec<(syn::Ident, Type)>,
    pub return_type: Type,
    pub stream_item_type: Option<Type>,
    pub doc_attrs: Vec<syn::Attribute>,
}

impl MethodInfo {
    /// Extract method info from an impl function
    pub fn from_fn(method: &ImplItemFn) -> syn::Result<Self> {
        let fn_name = method.sig.ident.clone();
        let method_name = fn_name.to_string();

        // Extract doc attributes and description
        let mut doc_lines = Vec::new();
        let mut doc_attrs = Vec::new();
        for attr in &method.attrs {
            if attr.path().is_ident("doc") {
                doc_attrs.push(attr.clone());
                if let Meta::NameValue(MetaNameValue { value, .. }) = &attr.meta {
                    if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = value {
                        doc_lines.push(s.value().trim().to_string());
                    }
                }
            }
        }
        let description = doc_lines.join(" ");

        // Extract parameters after &self
        let mut params = Vec::new();
        for arg in &method.sig.inputs {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(ident) = &*pat_type.pat {
                    params.push((ident.ident.clone(), (*pat_type.ty).clone()));
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
            doc_attrs,
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
