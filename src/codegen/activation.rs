//! Generate Activation trait implementation and RPC server

use crate::parse::MethodInfo;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use uuid::Uuid;

pub fn generate(
    struct_name: &syn::Ident,
    namespace: &str,
    version: &str,
    description: &str,
    long_description: Option<&str>,
    methods: &[MethodInfo],
    crate_path: &syn::Path,
    resolve_handle: bool,
    hub: bool,
    plugin_id: Option<&str>,
) -> TokenStream {
    let enum_name = format_ident!("{}Method", struct_name);
    let rpc_trait_name = format_ident!("{}Rpc", struct_name);
    let rpc_server_name = format_ident!("{}RpcServer", struct_name);
    let method_names: Vec<&str> = methods.iter().map(|m| m.method_name.as_str()).collect();

    let dispatch_arms = generate_dispatch_arms(methods, namespace, crate_path);
    let help_arms = generate_help_arms(methods);
    let rpc_trait_methods = generate_rpc_trait_methods(methods);
    let rpc_impl_methods = generate_rpc_impl_methods(struct_name, methods, namespace, crate_path);

    // Conditionally generate resolve_handle method
    let resolve_handle_impl = if resolve_handle {
        quote! {
            async fn resolve_handle(
                &self,
                handle: &#crate_path::types::Handle,
            ) -> Result<#crate_path::plexus::PlexusStream, #crate_path::plexus::PlexusError> {
                self.resolve_handle_impl(handle).await
            }
        }
    } else {
        quote! {}
    };

    // Generate call() fallback - hub routes to children, leaf returns error
    let call_fallback = if hub {
        quote! {
            #crate_path::plexus::route_to_child(self, method, params).await
        }
    } else {
        quote! {
            Err(#crate_path::plexus::PlexusError::MethodNotFound {
                activation: #namespace.to_string(),
                method: method.to_string(),
            })
        }
    };

    // Generate long_description() method
    let long_description_impl = if let Some(long_desc) = long_description {
        quote! {
            fn long_description(&self) -> Option<&str> { Some(#long_desc) }
        }
    } else {
        quote! {
            fn long_description(&self) -> Option<&str> { None }
        }
    };

    // Generate plugin_schema body - hub vs leaf, with or without long_description
    let plugin_schema_body = match (hub, long_description) {
        (true, Some(long_desc)) => {
            // Hub with long description
            quote! {
                #crate_path::plexus::PluginSchema::hub_with_long_description(
                    #namespace,
                    #version,
                    #description,
                    #long_desc,
                    #enum_name::method_schemas(),
                    self.plugin_children(),
                )
            }
        }
        (true, None) => {
            // Hub without long description
            quote! {
                #crate_path::plexus::PluginSchema::hub(
                    #namespace,
                    #version,
                    #description,
                    #enum_name::method_schemas(),
                    self.plugin_children(),
                )
            }
        }
        (false, Some(long_desc)) => {
            // Leaf with long description
            quote! {
                #crate_path::plexus::PluginSchema::leaf_with_long_description(
                    #namespace,
                    #version,
                    #description,
                    #long_desc,
                    #enum_name::method_schemas(),
                )
            }
        }
        (false, None) => {
            // Leaf without long description
            quote! {
                #crate_path::plexus::PluginSchema::leaf(
                    #namespace,
                    #version,
                    #description,
                    #enum_name::method_schemas(),
                )
            }
        }
    };

    // Generate plugin_id - either from explicit value or deterministically from namespace+major_version
    // Using only major version ensures handles survive minor/patch upgrades (semver compatibility)
    let plugin_id_str = plugin_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Extract major version (first component before '.')
            let major_version = version.split('.').next().unwrap_or("0");
            // Generate deterministic UUID v5 from namespace@major_version
            let name = format!("{}@{}", namespace, major_version);
            Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()).to_string()
        });

    quote! {
        impl #struct_name {
            pub const NAMESPACE: &'static str = #namespace;
            /// Stable plugin instance ID for handle routing
            pub const PLUGIN_ID: uuid::Uuid = uuid::uuid!(#plugin_id_str);

            /// Get the plugin's stable instance ID
            pub fn plugin_id(&self) -> uuid::Uuid {
                Self::PLUGIN_ID
            }
        }

        // Generate the RPC trait for jsonrpsee
        #[jsonrpsee::proc_macros::rpc(server, namespace = #namespace)]
        pub trait #rpc_trait_name {
            #(#rpc_trait_methods)*
        }

        // Implement the RPC server trait
        #[async_trait::async_trait]
        impl #rpc_server_name for #struct_name {
            #(#rpc_impl_methods)*
        }

        #[async_trait::async_trait]
        impl #crate_path::plexus::Activation for #struct_name {
            type Methods = #enum_name;

            fn namespace(&self) -> &str { #namespace }
            fn version(&self) -> &str { #version }
            fn description(&self) -> &str { #description }
            #long_description_impl

            fn methods(&self) -> Vec<&str> {
                vec![#(#method_names,)* "schema"]
            }

            fn method_help(&self, method: &str) -> Option<String> {
                match method {
                    #(#help_arms)*
                    "schema" => Some("Get plugin or method schema. Pass {\"method\": \"name\"} for a specific method.".to_string()),
                    _ => None,
                }
            }

            async fn call(
                &self,
                method: &str,
                params: serde_json::Value,
            ) -> Result<#crate_path::plexus::PlexusStream, #crate_path::plexus::PlexusError> {
                // Try local methods first
                match method {
                    #(#dispatch_arms)*
                    "schema" => {
                        // Check if a specific method was requested
                        let method_name: Option<String> = params.get("method")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let plugin_schema = self.plugin_schema();

                        let result = if let Some(ref name) = method_name {
                            // Find the specific method
                            plugin_schema.methods.iter()
                                .find(|m| m.name == *name)
                                .map(|m| #crate_path::plexus::SchemaResult::Method(m.clone()))
                                .ok_or_else(|| #crate_path::plexus::PlexusError::MethodNotFound {
                                    activation: #namespace.to_string(),
                                    method: name.clone(),
                                })?
                        } else {
                            // Return full plugin schema
                            #crate_path::plexus::SchemaResult::Plugin(plugin_schema)
                        };

                        Ok(#crate_path::plexus::wrap_stream(
                            futures::stream::once(async move { result }),
                            concat!(#namespace, ".schema"),
                            vec![#namespace.into()]
                        ))
                    }
                    _ => {
                        // Check for {method}.schema pattern (e.g., "echo.schema")
                        // Only if the prefix is an actual local method (not a child)
                        if let Some(method_name) = method.strip_suffix(".schema") {
                            let plugin_schema = self.plugin_schema();
                            if let Some(m) = plugin_schema.methods.iter().find(|m| m.name == method_name) {
                                let result = #crate_path::plexus::SchemaResult::Method(m.clone());
                                return Ok(#crate_path::plexus::wrap_stream(
                                    futures::stream::once(async move { result }),
                                    concat!(#namespace, ".method_schema"),
                                    vec![#namespace.into()]
                                ));
                            }
                            // Not a local method - fall through to child routing
                        }

                        // For hubs: try routing to child plugin via ChildRouter trait
                        // For leaves: return MethodNotFound
                        #call_fallback
                    }
                }
            }

            fn into_rpc_methods(self) -> jsonrpsee::core::server::Methods {
                self.into_rpc().into()
            }

            fn plugin_schema(&self) -> #crate_path::plexus::PluginSchema {
                #plugin_schema_body
            }

            #resolve_handle_impl
        }
    }
}

fn generate_rpc_trait_methods(methods: &[MethodInfo]) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let method_name = &m.fn_name;
            let method_name_str = &m.method_name;
            let unsubscribe_name = format!("unsubscribe_{}", method_name_str);
            let doc = &m.description;

            let params: Vec<TokenStream> = m
                .params
                .iter()
                .map(|p| {
                    let name = &p.name;
                    let ty = &p.ty;
                    quote! { #name: #ty }
                })
                .collect();

            quote! {
                #[doc = #doc]
                #[subscription(name = #method_name_str, unsubscribe = #unsubscribe_name, item = serde_json::Value)]
                async fn #method_name(&self, #(#params),*) -> jsonrpsee::core::SubscriptionResult;
            }
        })
        .collect()
}

fn generate_rpc_impl_methods(
    struct_name: &syn::Ident,
    methods: &[MethodInfo],
    namespace: &str,
    crate_path: &syn::Path,
) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let method_name = &m.fn_name;
            let method_name_str = &m.method_name;
            let param_names: Vec<&syn::Ident> = m.params.iter().map(|p| &p.name).collect();

            let params_with_types: Vec<TokenStream> = m
                .params
                .iter()
                .map(|p| {
                    let name = &p.name;
                    let ty = &p.ty;
                    quote! { #name: #ty }
                })
                .collect();

            if m.is_override {
                // Override method: returns Result<PlexusStream, _> directly
                // Forward the stream without additional wrapping
                quote! {
                    async fn #method_name(
                        &self,
                        pending: jsonrpsee::PendingSubscriptionSink,
                        #(#params_with_types),*
                    ) -> jsonrpsee::core::SubscriptionResult {
                        use futures::StreamExt;

                        let sink = pending.accept().await?;
                        let stream_result = #struct_name::#method_name(self, #(#param_names),*).await;

                        tokio::spawn(async move {
                            match stream_result {
                                Ok(mut stream) => {
                                    while let Some(item) = stream.next().await {
                                        if let Ok(raw_value) = serde_json::value::to_raw_value(&item) {
                                            if sink.send(raw_value).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    let error = #crate_path::plexus::PlexusStreamItem::Error {
                                        metadata: #crate_path::plexus::StreamMetadata::new(
                                            vec![#namespace.into()],
                                            #crate_path::plexus::PlexusContext::hash(),
                                        ),
                                        message: e.to_string(),
                                        code: None,
                                        recoverable: false,
                                    };
                                    if let Ok(raw_value) = serde_json::value::to_raw_value(&error) {
                                        let _ = sink.send(raw_value).await;
                                    }
                                }
                            }
                            // Send done event
                            let done = #crate_path::plexus::PlexusStreamItem::Done {
                                metadata: #crate_path::plexus::StreamMetadata::new(
                                    vec![#namespace.into()],
                                    #crate_path::plexus::PlexusContext::hash(),
                                ),
                            };
                            if let Ok(raw_value) = serde_json::value::to_raw_value(&done) {
                                let _ = sink.send(raw_value).await;
                            }
                        });

                        Ok(())
                    }
                }
            } else {
                // Normal method: wrap with wrap_stream
                let content_type = format!("{}.{}", namespace, method_name_str);

                quote! {
                    async fn #method_name(
                        &self,
                        pending: jsonrpsee::PendingSubscriptionSink,
                        #(#params_with_types),*
                    ) -> jsonrpsee::core::SubscriptionResult {
                        use futures::StreamExt;

                        let sink = pending.accept().await?;
                        let stream = #struct_name::#method_name(self, #(#param_names),*).await;
                        let wrapped = #crate_path::plexus::wrap_stream(
                            stream,
                            #content_type,
                            vec![#namespace.into()]
                        );

                        tokio::spawn(async move {
                            let mut stream = wrapped;
                            while let Some(item) = stream.next().await {
                                if let Ok(raw_value) = serde_json::value::to_raw_value(&item) {
                                    if sink.send(raw_value).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            // Send done event
                            let done = #crate_path::plexus::PlexusStreamItem::Done {
                                metadata: #crate_path::plexus::StreamMetadata::new(
                                    vec![#namespace.into()],
                                    #crate_path::plexus::PlexusContext::hash(),
                                ),
                            };
                            if let Ok(raw_value) = serde_json::value::to_raw_value(&done) {
                                let _ = sink.send(raw_value).await;
                            }
                        });

                        Ok(())
                    }
                }
            }
        })
        .collect()
}

fn generate_dispatch_arms(
    methods: &[MethodInfo],
    namespace: &str,
    crate_path: &syn::Path,
) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let method_name = &m.method_name;
            let fn_name = &m.fn_name;

            // Generate param extraction code
            let (param_extraction, param_names) = generate_param_extraction(m, crate_path);

            if m.is_override {
                // Override method: call directly, return Result<PlexusStream, _> as-is
                quote! {
                    #method_name => {
                        #param_extraction
                        self.#fn_name(#(#param_names),*).await
                    }
                }
            } else {
                // Normal method: wrap with wrap_stream
                let content_type = format!("{}.{}", namespace, method_name);
                quote! {
                    #method_name => {
                        #param_extraction
                        let stream = self.#fn_name(#(#param_names),*).await;
                        Ok(#crate_path::plexus::wrap_stream(
                            stream,
                            #content_type,
                            vec![#namespace.into()]
                        ))
                    }
                }
            }
        })
        .collect()
}

/// Generate param extraction code and return the param names for the call
fn generate_param_extraction<'a>(m: &'a MethodInfo, crate_path: &syn::Path) -> (TokenStream, Vec<&'a syn::Ident>) {
    let param_names: Vec<&syn::Ident> = m.params.iter().map(|p| &p.name).collect();

    let extraction = match m.params.len() {
        0 => quote! {},
        1 => {
            let param = &m.params[0];
            let param_name = &param.name;
            let param_str = param_name.to_string();
            let is_option = crate::codegen::method_enum::is_option_type(&param.ty);

            if is_option {
                // For Option<T>, missing field = None (not an error)
                quote! {
                    let #param_name = match &params {
                        serde_json::Value::Object(map) => {
                            map.get(#param_str)
                                .map(|v| serde_json::from_value(v.clone()))
                                .transpose()
                                .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?
                        }
                        serde_json::Value::Null => None,
                        _ => serde_json::from_value(params.clone())
                            .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?,
                    };
                }
            } else {
                // For required param, try to extract from object or deserialize directly
                quote! {
                    let #param_name = match &params {
                        serde_json::Value::Object(map) => {
                            if let Some(val) = map.get(#param_str) {
                                serde_json::from_value(val.clone())
                                    .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?
                            } else {
                                return Err(#crate_path::plexus::PlexusError::InvalidParams(
                                    format!("missing field: {}", #param_str)
                                ));
                            }
                        }
                        _ => serde_json::from_value(params.clone())
                            .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?,
                    };
                }
            }
        }
        _ => {
            let extractions: Vec<TokenStream> = m
                .params
                .iter()
                .map(|p| {
                    let name = &p.name;
                    let name_str = name.to_string();
                    let is_option = crate::codegen::method_enum::is_option_type(&p.ty);

                    if is_option {
                        // For Option<T>, missing field = None (not an error)
                        quote! {
                            let #name = map.get(#name_str)
                                .map(|v| serde_json::from_value(v.clone()))
                                .transpose()
                                .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?;
                        }
                    } else {
                        // For required fields, error on missing
                        quote! {
                            let #name = map.get(#name_str)
                                .ok_or_else(|| #crate_path::plexus::PlexusError::InvalidParams(
                                    format!("missing field: {}", #name_str)
                                ))
                                .and_then(|v| serde_json::from_value(v.clone())
                                    .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string())))?;
                        }
                    }
                })
                .collect();

            quote! {
                let map = params.as_object()
                    .ok_or_else(|| #crate_path::plexus::PlexusError::InvalidParams(
                        "expected object".to_string()
                    ))?;
                #(#extractions)*
            }
        }
    };

    (extraction, param_names)
}

fn generate_help_arms(methods: &[MethodInfo]) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let name = &m.method_name;
            let desc = &m.description;
            quote! { #name => Some(#desc.to_string()), }
        })
        .collect()
}
