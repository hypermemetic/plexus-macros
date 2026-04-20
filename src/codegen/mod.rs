//! Code generation for Plexus RPC macros

mod activation;
mod method_enum;

use crate::parse::{
    extract_doc_description, find_and_validate_list_search_method, has_child_attr,
    has_method_attr, ChildMethodInfo, ChildMethodKind, HubMethodAttrs, HubMethodsAttrs,
    ListSearchKind, ListSearchMethodInfo, MethodInfo,
};
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

    // Extract all #[method] / #[hub_method] functions and all #[child] functions.
    let mut methods: Vec<MethodInfo> = Vec::new();
    let mut child_methods: Vec<ChildMethodInfo> = Vec::new();

    for item in &mut input_impl.items {
        if let ImplItem::Fn(method) = item {
            // CHILD-3: a method annotated with both `#[child]` and `#[method]`
            // is a user error — the two roles are mutually exclusive.
            if has_child_attr(method) && has_method_attr(method) {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "#[plexus_macros::child] and #[plexus_macros::method] are mutually exclusive \
                     on the same function: a function is either an RPC method or a child router \
                     entry, not both",
                ));
            }

            // CHILD-3: collect + validate `#[child]` methods. These are NOT
            // exposed as RPC methods; they contribute only to ChildRouter
            // routing. Strip the attribute after parsing so the emitted impl
            // block doesn't still carry the (now-no-op) `#[child]` macro
            // invocation.
            if has_child_attr(method) {
                child_methods.push(ChildMethodInfo::from_fn(method)?);
                method.attrs.retain(|a| {
                    let last = a.path().segments.last().map(|s| s.ident.to_string());
                    !matches!(last.as_deref(), Some("child"))
                });
                continue;
            }

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

    // CHILD-3: at most one dynamic `#[child]` method per impl.
    let dynamic_children: Vec<&ChildMethodInfo> = child_methods
        .iter()
        .filter(|c| matches!(c.kind, ChildMethodKind::Dynamic))
        .collect();
    if dynamic_children.len() > 1 {
        let names: Vec<String> = dynamic_children
            .iter()
            .map(|c| c.fn_name.to_string())
            .collect();
        return Err(syn::Error::new_spanned(
            &input_impl,
            format!(
                "at most one dynamic #[child] method is allowed per activation impl; \
                 found {}: {}",
                dynamic_children.len(),
                names.join(", ")
            ),
        ));
    }

    // CHILD-3: reject mixing the legacy `children = [...]` attribute with
    // the new `#[child]` method attribute on the same impl — the two
    // systems don't merge, and silently picking one would hide intent.
    if !child_methods.is_empty() && !args.children.is_empty() {
        return Err(syn::Error::new_spanned(
            &input_impl,
            "#[plexus_macros::child] methods cannot be combined with the legacy \
             `children = [...]` activation attribute on the same impl; choose one. \
             Remove `children = [...]` to migrate to the method-based syntax.",
        ));
    }

    // CHILD-4: resolve `list = "..."` / `search = "..."` args against the
    // same-impl sibling methods. Missing names produce `not found in impl`;
    // signature mismatches produce an error containing `signature mismatch`.
    let mut list_method: Option<ListSearchMethodInfo> = None;
    let mut search_method: Option<ListSearchMethodInfo> = None;
    for child in &child_methods {
        if let Some(name) = &child.list_fn {
            let info = find_and_validate_list_search_method(
                &input_impl.items,
                name,
                ListSearchKind::List,
                &child.fn_name,
            )?;
            if list_method.is_some() {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "duplicate `list = \"{}\"` across #[plexus_macros::child] methods; \
                         at most one list method per activation impl",
                        name
                    ),
                ));
            }
            list_method = Some(info);
        }
        if let Some(name) = &child.search_fn {
            let info = find_and_validate_list_search_method(
                &input_impl.items,
                name,
                ListSearchKind::Search,
                &child.fn_name,
            )?;
            if search_method.is_some() {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "duplicate `search = \"{}\"` across #[plexus_macros::child] methods; \
                         at most one search method per activation impl",
                        name
                    ),
                ));
            }
            search_method = Some(info);
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
        &child_methods,
        list_method.as_ref(),
        search_method.as_ref(),
    );

    Ok(quote! {
        #input_impl
        #method_enum
        #activation_impl
    })
}
