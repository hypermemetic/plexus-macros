//! Hub Method Macro
//!
//! Proc macro for defining hub methods where the function signature IS the schema.
//!
//! # Example
//!
//! ```ignore
//! use hub_macro::hub_method;
//!
//! /// Execute a bash command
//! #[hub_method]
//! async fn execute(input: ExecuteInput) -> Recv<BashEvent, Done> {
//!     // implementation
//! }
//! ```
//!
//! The macro extracts:
//! - Method name from function name
//! - Description from doc comments
//! - Input schema from parameter type
//! - Protocol schema from return type

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, punctuated::Punctuated, Expr, ExprLit, FnArg, ItemFn, Lit, Meta,
    MetaNameValue, Pat, ReturnType, Token, Type,
};

/// Parsed attributes for hub_method
struct HubMethodAttrs {
    name: Option<String>,
    /// Base crate path for imports (default: "crate")
    /// Use "substrate" when testing from outside the crate
    crate_path: String,
}

impl Parse for HubMethodAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut crate_path = "crate".to_string();

        if !input.is_empty() {
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;

            for meta in metas {
                if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                    if path.is_ident("name") {
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            name = Some(s.value());
                        }
                    } else if path.is_ident("crate_path") {
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }) = value
                        {
                            crate_path = s.value();
                        }
                    }
                }
            }
        }

        Ok(HubMethodAttrs { name, crate_path })
    }
}

/// Attribute macro for hub methods.
///
/// Extracts schema information from the function signature and generates
/// registration code.
///
/// # Attributes
///
/// - `name = "custom_name"` - Override the method name (default: function name)
///
/// # Example
///
/// ```ignore
/// /// Execute a bash command and stream output
/// #[hub_method]
/// async fn execute(input: ExecuteInput) -> Session![
///     loop {
///         choose {
///             0 => { send BashEvent; continue; },
///             1 => { send ExitCode; break; },
///         }
///     }
/// ] {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn hub_method(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as HubMethodAttrs);
    let input_fn = parse_macro_input!(item as ItemFn);

    match hub_method_impl(args, input_fn) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn hub_method_impl(args: HubMethodAttrs, input_fn: ItemFn) -> syn::Result<TokenStream2> {
    // Extract method name (from attr or function name)
    let method_name = args
        .name
        .unwrap_or_else(|| input_fn.sig.ident.to_string());

    // Extract description from doc comments
    let description = extract_doc_comment(&input_fn);

    // Extract input type from first parameter (if any)
    let input_type = extract_input_type(&input_fn)?;

    // Extract return type (the session protocol)
    let return_type = extract_return_type(&input_fn)?;

    // Function name for the schema function
    let fn_name = &input_fn.sig.ident;
    let schema_fn_name = format_ident!("{}_schema", fn_name);

    // Parse crate path
    let crate_path: syn::Path = syn::parse_str(&args.crate_path)
        .map_err(|e| syn::Error::new_spanned(&input_fn.sig, format!("Invalid crate_path: {}", e)))?;

    // Generate the schema function
    let schema_fn = generate_schema_fn(
        &schema_fn_name,
        &method_name,
        &description,
        input_type.as_ref(),
        &return_type,
        &crate_path,
    );

    // Return the original function plus the schema function
    Ok(quote! {
        #input_fn

        #schema_fn
    })
}

fn extract_doc_comment(input_fn: &ItemFn) -> String {
    let mut doc_lines = Vec::new();

    for attr in &input_fn.attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(MetaNameValue { value, .. }) = &attr.meta {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = value
                {
                    doc_lines.push(s.value().trim().to_string());
                }
            }
        }
    }

    doc_lines.join(" ")
}

fn extract_input_type(input_fn: &ItemFn) -> syn::Result<Option<Type>> {
    // Skip self parameter, get first real parameter
    for arg in &input_fn.sig.inputs {
        match arg {
            FnArg::Receiver(_) => continue, // Skip &self
            FnArg::Typed(pat_type) => {
                // Skip context-like parameters (could be configurable)
                if let Pat::Ident(ident) = &*pat_type.pat {
                    let name = ident.ident.to_string();
                    if name == "ctx" || name == "context" || name == "self_" {
                        continue;
                    }
                }
                return Ok(Some((*pat_type.ty).clone()));
            }
        }
    }

    Ok(None) // No input parameter
}

fn extract_return_type(input_fn: &ItemFn) -> syn::Result<Type> {
    match &input_fn.sig.output {
        ReturnType::Default => Err(syn::Error::new_spanned(
            &input_fn.sig,
            "hub_method requires a return type (the session protocol)",
        )),
        ReturnType::Type(_, ty) => Ok((*ty.clone()).clone()),
    }
}

fn generate_schema_fn(
    fn_name: &syn::Ident,
    method_name: &str,
    description: &str,
    input_type: Option<&Type>,
    return_type: &Type,
    crate_path: &syn::Path,
) -> TokenStream2 {
    let input_schema = if let Some(input_ty) = input_type {
        quote! {
            Some(serde_json::to_value(schemars::schema_for!(#input_ty)).unwrap())
        }
    } else {
        quote! { None }
    };

    // The return type is the server's protocol after receiving input
    // Full server protocol: if input exists, Recv<Input, ReturnType>, else just ReturnType
    let server_protocol = if input_type.is_some() {
        quote! {
            {
                let input_schema = #input_schema;
                let continuation = <#return_type as SessionSchema>::schema();
                ProtocolSchema::Recv {
                    payload: input_schema.unwrap(),
                    then: Box::new(continuation),
                }
            }
        }
    } else {
        quote! {
            <#return_type as SessionSchema>::schema()
        }
    };

    // Client protocol is the dual
    let client_protocol = if input_type.is_some() {
        quote! {
            {
                let input_schema = #input_schema;
                // Client sends input, then does dual of return type
                let continuation = <<#return_type as dialectic::Session>::Dual as SessionSchema>::schema();
                ProtocolSchema::Send {
                    payload: input_schema.unwrap(),
                    then: Box::new(continuation),
                }
            }
        }
    } else {
        quote! {
            <<#return_type as dialectic::Session>::Dual as SessionSchema>::schema()
        }
    };

    quote! {
        /// Generated schema function for this hub method
        pub fn #fn_name() -> #crate_path::plexus::MethodSchema {
            use #crate_path::plexus::{MethodSchema, ProtocolSchema, SessionSchema};

            MethodSchema {
                name: #method_name.to_string(),
                description: #description.to_string(),
                protocol: #client_protocol,
                server_protocol: #server_protocol,
            }
        }
    }
}
