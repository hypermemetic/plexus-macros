//! Code generation for hub macros

mod activation;
mod method_enum;

use crate::parse::{HubMethodAttrs, HubMethodsAttrs, MethodInfo};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItem, ItemImpl, Meta, Type};

/// Generate all code from a #[hub_methods] impl block
pub fn generate_all(args: HubMethodsAttrs, mut input_impl: ItemImpl) -> syn::Result<TokenStream> {
    let crate_path: syn::Path = syn::parse_str(&args.crate_path)?;

    // Get struct name
    let struct_name = match &*input_impl.self_ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|s| s.ident.clone())
            .ok_or_else(|| syn::Error::new_spanned(&input_impl.self_ty, "Expected struct name"))?,
        _ => return Err(syn::Error::new_spanned(&input_impl.self_ty, "Expected struct type")),
    };

    // Extract all #[hub_method] functions
    let mut methods: Vec<MethodInfo> = Vec::new();

    for item in &mut input_impl.items {
        if let ImplItem::Fn(method) = item {
            // Check for hub_method attribute (handles both #[hub_method] and #[hub_macro::hub_method])
            let hub_method_idx = method.attrs.iter().position(|attr| {
                let path = attr.path();
                path.is_ident("hub_method")
                    || path.segments.last().map(|s| s.ident == "hub_method").unwrap_or(false)
            });

            if let Some(idx) = hub_method_idx {
                // Parse the attribute contents before removing
                let attr = method.attrs.remove(idx);
                let hub_method_attrs = match &attr.meta {
                    Meta::Path(_) => None, // #[hub_method] with no args
                    Meta::List(list) => {
                        Some(syn::parse2::<HubMethodAttrs>(list.tokens.clone())?)
                    }
                    Meta::NameValue(_) => None,
                };
                methods.push(MethodInfo::from_fn(method, hub_method_attrs.as_ref())?);
            }
        }
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            &input_impl,
            "No #[hub_method] functions found",
        ));
    }

    // Generate pieces
    let method_enum = method_enum::generate(&struct_name, &methods, &crate_path);
    let activation_impl = activation::generate(
        &struct_name,
        &args.namespace,
        &args.version,
        args.description.as_deref().unwrap_or(&format!("{} activation", args.namespace)),
        &methods,
        &crate_path,
        args.resolve_handle,
    );

    Ok(quote! {
        #input_impl
        #method_enum
        #activation_impl
    })
}
