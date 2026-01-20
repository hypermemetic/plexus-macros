//! Hub Method Macro
//!
//! Proc macro for defining hub methods where the function signature IS the schema.
//!
//! # Example
//!
//! ```ignore
//! use hub_macro::{hub_methods, hub_method};
//!
//! #[hub_methods(namespace = "bash", version = "1.0.0")]
//! impl Bash {
//!     /// Execute a bash command
//!     #[hub_method]
//!     async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
//!         // implementation
//!     }
//! }
//! ```
//!
//! The macro extracts:
//! - Method name from function name
//! - Description from doc comments
//! - Input schema from parameter types
//! - Return type schema from Stream Item type

mod codegen;
mod handle_enum;
mod parse;
mod stream_event;

use codegen::generate_all;
use parse::HubMethodsAttrs;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, punctuated::Punctuated, Expr, ExprLit, FnArg, ItemFn, ItemImpl, Lit, Meta,
    MetaNameValue, Pat, ReturnType, Token, Type,
};

/// Parsed attributes for hub_method (standalone version)
struct HubMethodAttrs {
    name: Option<String>,
    /// Base crate path for imports (default: "crate")
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

/// Attribute macro for hub methods within an impl block.
///
/// This is used inside a `#[hub_methods]` impl block to mark individual methods.
/// When used standalone, it generates a schema function.
///
/// # Example
///
/// ```ignore
/// #[hub_methods(namespace = "bash")]
/// impl Bash {
///     /// Execute a bash command
///     #[hub_method]
///     async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
///         // ...
///     }
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

    // Extract return type
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
                // Skip context-like parameters
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

    Ok(None)
}

fn extract_return_type(input_fn: &ItemFn) -> syn::Result<Type> {
    match &input_fn.sig.output {
        ReturnType::Default => Err(syn::Error::new_spanned(
            &input_fn.sig,
            "hub_method requires a return type",
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

    let _ = return_type; // Will be used for protocol schema in future
    let _ = crate_path;

    quote! {
        /// Generated schema function for this hub method
        #[allow(dead_code)]
        pub fn #fn_name() -> serde_json::Value {
            serde_json::json!({
                "name": #method_name,
                "description": #description,
                "input": #input_schema,
            })
        }
    }
}

/// Attribute macro for impl blocks containing hub methods.
///
/// Generates:
/// - Method enum for schema extraction
/// - Activation trait implementation
/// - RPC server trait and implementation
///
/// # Attributes
///
/// - `namespace = "..."` (required) - The activation namespace
/// - `version = "..."` (optional, default: "1.0.0") - Version string
/// - `description = "..."` (optional) - Activation description
/// - `crate_path = "..."` (optional, default: "crate") - Path to substrate crate
///
/// # Example
///
/// ```ignore
/// #[hub_methods(namespace = "bash", version = "1.0.0", description = "Execute bash commands")]
/// impl Bash {
///     /// Execute a bash command and stream output
///     #[hub_method]
///     async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> + Send + 'static {
///         self.executor.execute(&command).await
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn hub_methods(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as HubMethodsAttrs);
    let input_impl = parse_macro_input!(item as ItemImpl);

    match generate_all(args, input_impl) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// **DEPRECATED**: This derive macro is no longer needed.
///
/// With the caller-wraps streaming architecture, event types no longer need to
/// implement `ActivationStreamItem`. Just use plain domain types with standard
/// derives:
///
/// ```ignore
/// #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
/// #[serde(tag = "event", rename_all = "snake_case")]
/// pub enum MyEvent {
///     Data { value: String },
///     Complete { result: i32 },
/// }
/// ```
///
/// The wrapping happens at the call site via `wrap_stream()`.
#[deprecated(
    since = "0.2.0",
    note = "No longer needed - use plain domain types with Serialize/Deserialize"
)]
#[proc_macro_derive(StreamEvent, attributes(stream_event, terminal))]
pub fn stream_event_derive(input: TokenStream) -> TokenStream {
    stream_event::derive(input)
}

/// Derive macro for type-safe handle creation and parsing.
///
/// Generates:
/// - `to_handle(&self) -> Handle` - converts enum variant to Handle
/// - `impl TryFrom<&Handle>` - parses Handle back to enum variant
/// - `impl From<EnumName> for Handle` - convenience conversion
///
/// # Attributes
///
/// Enum-level (required):
/// - `plugin_id = "CONSTANT_NAME"` - Name of the constant holding the plugin UUID
/// - `version = "1.0.0"` - Semantic version for handles
/// - `crate_path = "..."` (optional, default: "hub_core") - Path to hub_core crate
///
/// Variant-level:
/// - `method = "..."` (required) - The handle.method value
/// - `table = "..."` (optional) - SQLite table name (for future resolution)
/// - `key = "..."` (optional) - Primary key column (for future resolution)
///
/// # Example
///
/// ```ignore
/// use hub_macro::HandleEnum;
/// use uuid::Uuid;
///
/// pub const MY_PLUGIN_ID: Uuid = uuid::uuid!("550e8400-e29b-41d4-a716-446655440000");
///
/// #[derive(HandleEnum)]
/// #[handle(plugin_id = "MY_PLUGIN_ID", version = "1.0.0")]
/// pub enum MyPluginHandle {
///     #[handle(method = "event", table = "events", key = "id")]
///     Event { event_id: String },
///
///     #[handle(method = "message")]
///     Message { message_id: String, role: String },
/// }
///
/// // Usage:
/// let handle_enum = MyPluginHandle::Event { event_id: "evt-123".into() };
/// let handle: Handle = handle_enum.to_handle();
/// // handle.method == "event"
/// // handle.meta == ["evt-123"]
///
/// // Parsing back:
/// let parsed = MyPluginHandle::try_from(&handle)?;
/// ```
#[proc_macro_derive(HandleEnum, attributes(handle))]
pub fn handle_enum_derive(input: TokenStream) -> TokenStream {
    handle_enum::derive(input)
}
