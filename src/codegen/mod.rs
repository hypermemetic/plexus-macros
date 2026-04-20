//! Code generation for Plexus RPC macros

mod activation;
mod method_enum;

use crate::parse::{extract_doc_description, HubMethodAttrs, HubMethodsAttrs, MethodInfo};
use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{FnArg, ImplItem, ItemImpl, Meta, Type};

/// Resolve the `crate_path` to a syn::Path.
///
/// If the caller passed `crate_path = "..."` explicitly, parse that string.
/// Otherwise, call `proc_macro_crate::crate_name("plexus-core")` and emit the
/// correct path:
///   - `FoundCrate::Itself`    → `crate`  (code being compiled IS plexus-core)
///   - `FoundCrate::Name(n)`   → `::<n>`  (external; respects renaming)
///   - `Err`                   → a compile error naming the missing dependency
fn resolve_crate_path(explicit: Option<&str>, err_span: Span) -> syn::Result<syn::Path> {
    if let Some(s) = explicit {
        return syn::parse_str(s);
    }

    match crate_name("plexus-core") {
        Ok(FoundCrate::Itself) => syn::parse_str("crate"),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, err_span);
            Ok(syn::parse_quote!(::#ident))
        }
        Err(e) => Err(syn::Error::new(
            err_span,
            format!(
                "#[plexus_macros::activation] could not locate the `plexus-core` \
                 dependency in Cargo.toml ({e}). Add `plexus-core` as a dependency, \
                 or pass `crate_path = \"...\"` explicitly to override."
            ),
        )),
    }
}

/// Generate all code from a #[hub_methods] impl block
pub fn generate_all(args: HubMethodsAttrs, mut input_impl: ItemImpl) -> syn::Result<TokenStream> {
    let crate_path: syn::Path =
        resolve_crate_path(args.crate_path.as_deref(), Span::call_site())?;

    // Get struct name (ident only, for enum naming)
    let struct_name = match &*input_impl.self_ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|s| s.ident.clone())
            .ok_or_else(|| syn::Error::new_spanned(&input_impl.self_ty, "Expected struct name"))?,
        _ => return Err(syn::Error::new_spanned(&input_impl.self_ty, "Expected struct type")),
    };

    // Get full self type (with generics) and impl generics for trait impl
    let self_ty = &input_impl.self_ty;
    let (impl_generics, _, where_clause) = input_impl.generics.split_for_impl();

    // Extract all #[method] / #[hub_method] functions
    let mut methods: Vec<MethodInfo> = Vec::new();

    for item in &mut input_impl.items {
        if let ImplItem::Fn(method) = item {
            // Accept both the canonical #[method] / #[plexus_macros::method] and
            // the deprecated #[hub_method] / #[plexus_macros::hub_method] spellings.
            let hub_method_idx = method.attrs.iter().position(|attr| {
                let path = attr.path();
                let last = path.segments.last().map(|s| s.ident.to_string());
                matches!(last.as_deref(), Some("method") | Some("hub_method"))
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

                // Strip #[from_auth(...)] and #[activation_param] attributes from parameters
                // so Rust doesn't complain about unknown attributes in the emitted code.
                for arg in &mut method.sig.inputs {
                    if let FnArg::Typed(pat_type) = arg {
                        pat_type.attrs.retain(|attr| {
                            !attr.path().is_ident("from_auth")
                                && !attr.path().is_ident("activation_param")
                        });
                    }
                }
            }
        }
    }

    if methods.is_empty() {
        return Err(syn::Error::new_spanned(
            &input_impl,
            "No #[plexus::method] functions found",
        ));
    }

    // Resolve the activation description with this precedence:
    //   1. Explicit `description = "..."` on #[plexus_macros::activation(...)] wins.
    //   2. Otherwise fall back to `///` doc comments on the impl block, joined with
    //      '\n' and with common leading whitespace stripped (same rule as `cargo doc`).
    //   3. If neither is present, description is the empty string.
    let resolved_description: String = args
        .description
        .clone()
        .or_else(|| extract_doc_description(&input_impl.attrs))
        .unwrap_or_default();

    // Generate pieces
    let method_enum = method_enum::generate(&struct_name, &methods, &crate_path);
    let activation_impl = activation::generate(
        &struct_name,
        self_ty,
        &impl_generics,
        where_clause,
        &args.namespace,
        args.version.as_deref(),
        &resolved_description,
        args.long_description.as_deref(),
        &methods,
        &crate_path,
        args.resolve_handle,
        args.hub,
        args.plugin_id.as_deref(),
        args.namespace_fn.as_deref(),
        args.request_type.as_ref(),
        &args.children,
    );

    Ok(quote! {
        #input_impl
        #method_enum
        #activation_impl
    })
}
