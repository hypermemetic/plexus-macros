//! Generate Activation trait implementation and RPC server

use crate::parse::MethodInfo;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(
    struct_name: &syn::Ident,
    namespace: &str,
    version: &str,
    description: &str,
    methods: &[MethodInfo],
    crate_path: &syn::Path,
) -> TokenStream {
    let enum_name = format_ident!("{}Method", struct_name);
    let rpc_trait_name = format_ident!("{}Rpc", struct_name);
    let rpc_server_name = format_ident!("{}RpcServer", struct_name);
    let method_names: Vec<&str> = methods.iter().map(|m| m.method_name.as_str()).collect();

    let dispatch_arms = generate_dispatch_arms(methods, namespace, crate_path);
    let help_arms = generate_help_arms(methods);
    let rpc_trait_methods = generate_rpc_trait_methods(methods);
    let rpc_impl_methods = generate_rpc_impl_methods(struct_name, methods, namespace, crate_path);

    quote! {
        impl #struct_name {
            pub const NAMESPACE: &'static str = #namespace;
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

            fn methods(&self) -> Vec<&str> {
                vec![#(#method_names),*]
            }

            fn method_help(&self, method: &str) -> Option<String> {
                match method {
                    #(#help_arms)*
                    _ => None,
                }
            }

            async fn call(
                &self,
                method: &str,
                params: serde_json::Value,
            ) -> Result<#crate_path::plexus::PlexusStream, #crate_path::plexus::PlexusError> {
                match method {
                    #(#dispatch_arms)*
                    _ => Err(#crate_path::plexus::PlexusError::MethodNotFound {
                        activation: #namespace.to_string(),
                        method: method.to_string(),
                    }),
                }
            }

            fn into_rpc_methods(self) -> jsonrpsee::core::server::Methods {
                self.into_rpc().into()
            }

            fn full_schema(&self) -> #crate_path::plexus::ActivationFullSchema {
                #crate_path::plexus::ActivationFullSchema {
                    namespace: #namespace.to_string(),
                    version: #version.to_string(),
                    description: #description.to_string(),
                    methods: #enum_name::method_schemas(),
                }
            }
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
    let _ = crate_path; // Will be used when we need crate-specific imports
    methods
        .iter()
        .map(|m| {
            let method_name = &m.fn_name;
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

            // Call the method directly and convert stream to subscription
            // Don't wrap in PlexusStream - the stream items implement ActivationStreamItem
            quote! {
                async fn #method_name(
                    &self,
                    pending: jsonrpsee::PendingSubscriptionSink,
                    #(#params_with_types),*
                ) -> jsonrpsee::core::SubscriptionResult {
                    use #crate_path::plugin_system::conversion::IntoSubscription;
                    let stream = #struct_name::#method_name(self, #(#param_names),*).await;
                    let provenance = #crate_path::plexus::Provenance::root(#namespace);
                    // Box::pin the stream to satisfy Unpin bound
                    Box::pin(stream).into_subscription(pending, provenance).await
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

            match m.params.len() {
                0 => quote! {
                    #method_name => {
                        let stream = self.#fn_name().await;
                        let provenance = #crate_path::plexus::Provenance::root(#namespace);
                        Ok(#crate_path::plexus::into_plexus_stream(stream, provenance))
                    }
                },
                1 => {
                    let param = &m.params[0];
                    let param_name = &param.name;
                    let param_str = param_name.to_string();
                    quote! {
                        #method_name => {
                            let #param_name = match &params {
                                serde_json::Value::Object(map) => {
                                    if let Some(val) = map.get(#param_str) {
                                        serde_json::from_value(val.clone())
                                            .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?
                                    } else {
                                        serde_json::from_value(params.clone())
                                            .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?
                                    }
                                }
                                _ => serde_json::from_value(params.clone())
                                    .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string()))?,
                            };
                            let stream = self.#fn_name(#param_name).await;
                            let provenance = #crate_path::plexus::Provenance::root(#namespace);
                            Ok(#crate_path::plexus::into_plexus_stream(stream, provenance))
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
                            quote! {
                                let #name = map.get(#name_str)
                                    .ok_or_else(|| #crate_path::plexus::PlexusError::InvalidParams(
                                        format!("missing field: {}", #name_str)
                                    ))
                                    .and_then(|v| serde_json::from_value(v.clone())
                                        .map_err(|e| #crate_path::plexus::PlexusError::InvalidParams(e.to_string())))?;
                            }
                        })
                        .collect();
                    let param_names: Vec<&syn::Ident> = m.params.iter().map(|p| &p.name).collect();

                    quote! {
                        #method_name => {
                            let map = params.as_object()
                                .ok_or_else(|| #crate_path::plexus::PlexusError::InvalidParams(
                                    "expected object".to_string()
                                ))?;
                            #(#extractions)*
                            let stream = self.#fn_name(#(#param_names),*).await;
                            let provenance = #crate_path::plexus::Provenance::root(#namespace);
                            Ok(#crate_path::plexus::into_plexus_stream(stream, provenance))
                        }
                    }
                }
            }
        })
        .collect()
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
