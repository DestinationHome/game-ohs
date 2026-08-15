use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Pat, parse_macro_input};

// ---------------------------------------------------------------------------
// Shared code-generation helpers
// ---------------------------------------------------------------------------

/// Regex pattern used to detect PS3 OHS client User-Agent prefixes.
///
/// Matches multiple firmware versions by checking for the common format.
const PS3_USER_AGENT_RE: &str = r"^PSHome PS3Application libhttp/\d\.\d\.\d-\d{3} \(CellOS\)";

/// Generates tokens that extract the `data` field from an
/// `actix_multipart::Multipart` payload, then either jamin-decode it
/// (normal PS3 client) or parse it as plain JSON (custom user-agent).
///
/// Assumes the surrounding scope has:
///   - `mut payload: actix_multipart::Multipart`
///   - `__jamin_req: actix_web::HttpRequest`
///
/// Produces bindings:
///   - `let __is_json_client: bool = …;`
///   - `let <var_ident>: <target_ty> = …;`
fn multipart_decode_tokens(
    var_ident: &Ident,
    target_ty: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let ps3_ua_re = PS3_USER_AGENT_RE;
    quote! {
        use ::actix_web::web::BytesMut;
        use ::futures::StreamExt;
        use ::once_cell::sync::Lazy;

        // Compile the regex once per module.
        static __PS3_UA_RE: Lazy<regex::Regex> = Lazy::new(|| {
            regex::Regex::new(#ps3_ua_re).expect("invalid PS3 UA regex")
        });

        // Check whether this request comes from the real PS3 client or a
        // custom tool (e.g. a debug client sending plain JSON). We treat a
        // non-matching UA as a JSON client.
        let __is_json_client: bool = __jamin_req
            .headers()
            .get(::actix_web::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|ua| !__PS3_UA_RE.is_match(ua))
            .unwrap_or(true);
        drop(__jamin_req);

        let mut data_field: Option<String> = None;
        while let Some(item) = payload.next().await {
            let mut field = item.map_err(|e| ::actix_web::error::ErrorBadRequest(e))?;
            let cd = field.content_disposition();
            if let Some(name) = cd.get_name() {
                if name == "data" {
                    let mut bytes = BytesMut::new();
                    while let Some(chunk) = field.next().await {
                        let chunk = chunk.map_err(|e| ::actix_web::error::ErrorBadRequest(e))?;
                        bytes.extend_from_slice(&chunk);
                    }
                    data_field = Some(
                        String::from_utf8(bytes.to_vec())
                            .map_err(|e| ::actix_web::error::ErrorBadRequest(e))?,
                    );
                    break;
                }
            }
        }

        let data_str = data_field
            .ok_or_else(|| ::actix_web::error::ErrorBadRequest("missing data field"))?;

        fn __jamin_strip_hash_prefix(text: &str) -> &str {
            let jamin_start = if text.len() >= 16
                && text[8..16].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                16 // hash + writeKey
            } else {
                8  // hash only
            };

            return &text[jamin_start..];
        }

        fn __jamin_try_decrypt_printable_text(text: &str) -> Option<String> {
            let ctx = ::jamin::cipher::CipherContext::new(::jamin::cipher::constants::CIPHER_VERSION_1.to_vec());

            let decoded = ctx.decode(text);
            ctx.decrypt(&decoded)
        }

        // Track whether the incoming payload required jamin decryption
        let mut __jamin_was_encrypted: bool = false;

        let decoded_json = if __is_json_client {
            // Custom client: the data field is already plain JSON.
            data_str.clone()
        } else {
            let stripped_data = __jamin_strip_hash_prefix(&data_str);
            let mut decoded_value = ::jamin::jamin::JaminDecoder::decode(stripped_data)
                .or_else(|| ::jamin::jamin::JaminDecoder::decode(&data_str));

            if decoded_value.is_none() {
                if let Some(decrypted) = __jamin_try_decrypt_printable_text(&data_str) {
                    __jamin_was_encrypted = true;
                    let decrypted_stripped = __jamin_strip_hash_prefix(&decrypted);
                    decoded_value = ::jamin::jamin::JaminDecoder::decode(decrypted_stripped)
                        .or_else(|| ::jamin::jamin::JaminDecoder::decode(&decrypted));
                }
            }

            let decoded_value = match decoded_value {
                Some(v) => v,
                None => {
                    ::tracing::error!(data = %data_str, "jamin decode failed");
                    return Err(::actix_web::error::ErrorBadRequest(
                        format!("decode error: {}", data_str),
                    ));
                }
            };

            ::serde_json::to_string(&decoded_value).map_err(|e| {
                ::tracing::error!(error = %e, "failed to serialize jamin value to json");
                ::actix_web::error::ErrorBadRequest(format!("serialize error: {}", e))
            })?
        };

        let #var_ident: #target_ty = ::serde_json::from_str(&decoded_json).map_err(|e| {
            ::tracing::error!(error = %e, json = %decoded_json, "failed to deserialize into target struct");
            ::actix_web::error::ErrorBadRequest(format!("deserialize error: {}", e))
        })?;
    }
}

/// Generates tokens that take a `Result<T, actix_web::Error>` expression,
/// then either:
///   - **PS3 client**: jamin-encode the `Ok` value, wrap in `<ohs>…</ohs>`.
///   - **Custom client**: return plain JSON.
///
/// Assumes `__is_json_client: bool` is in scope.
fn ohs_result_tokens(result_expr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        fn __jamin_try_encrypt_payload(plaintext: &str) -> Option<String> {
            let ctx = ::jamin::cipher::CipherContext::new(::jamin::cipher::constants::CIPHER_VERSION_1.to_vec());
            return ctx.encrypt(&plaintext, 420);
        }

        match #result_expr {
            Ok(v) => {
                let json_val = ::serde_json::to_value(&v)
                    .map_err(|e| ::actix_web::error::ErrorInternalServerError(e))?;

                if __is_json_client {
                    // Custom client: plain JSON response.
                    Ok(::actix_web::HttpResponse::Ok()
                        .content_type("application/json")
                        .json(::serde_json::json!({
                            "status": "success",
                            "value": json_val
                        })))
                } else {
                    // PS3 client: jamin-encoded <ohs> response.
                    let wrapper = ::serde_json::json!({
                        "status": "success",
                        "value": json_val
                    });
                    let encoded = jamin::jamin::JaminEncoder::encode_serde(&wrapper);
                    let body = if __jamin_was_encrypted {
                        if let Some(encrypted) = __jamin_try_encrypt_payload(&encoded) {
                            format!("<ohs>{}</ohs>", encrypted
                                .replace("&", "&amp;")
                                .replace("<", "&lt;")
                                .replace(">", "&gt;")
                            )
                        } else {
                            format!("<ohs>{}</ohs>", encoded
                                .replace("&", "&amp;")
                                .replace("<", "&lt;")
                                .replace(">", "&gt;")
                            )
                        }
                    } else {
                        // Request wasn't encrypted, return plain encoded payload.
                        format!("<ohs>{}</ohs>", encoded
                            .replace("&", "&amp;")
                            .replace("<", "&lt;")
                            .replace(">", "&gt;")
                        )
                    };
                    Ok(::actix_web::HttpResponse::Ok()
                        .content_type("application/xml")
                        .body(body))
                }
            }
            Err(e) => Err(e),
        }
    }
}

/// Collects all named-parameter idents from a function signature.
fn collect_param_idents(sig: &syn::Signature) -> Vec<Ident> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pt) = arg
                && let Pat::Ident(pi) = &*pt.pat
            {
                return Some(pi.ident.clone());
            }
            None
        })
        .collect()
}

/// Builds a `name -> type` map from a function signature.
fn param_type_map(sig: &syn::Signature) -> std::collections::HashMap<String, syn::Type> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pt) = arg
                && let Pat::Ident(pi) = &*pt.pat
            {
                return Some((pi.ident.to_string(), (*pt.ty).clone()));
            }
            None
        })
        .collect()
}

// ---------------------------------------------------------------------------
// #[jamin_handler]
// ---------------------------------------------------------------------------

/// Attribute macro that rewrites an async handler so that:
///
/// 1. A multipart `data` field is extracted and jamin-decoded into a struct
///    whose fields correspond to the *captured* parameters listed in the
///    attribute (e.g. `#[jamin_handler(user)]`).
/// 2. The handler's return value (`Result<T: Serialize, actix_web::Error>`)
///    is jamin-encoded, wrapped in `<ohs>…</ohs>`, and returned as an
///    `HttpResponse`.
/// 3. Optionally, a registry wrapper and auto-registration hook are generated
///    so the handler can also be invoked via the batch registry.  Pass
///    `no_registry` to suppress this.
/// 4. The registry name defaults to the function name, but can be overridden with `name=…` (e.g. `#[jamin_handler(name="foo")]`).
///
/// The attributed function must be `async` and return `Result<T, actix_web::Error>`.
#[proc_macro_attribute]
pub fn jamin_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // -- Parse attribute flags -------------------------------------------
    let attr_str = attr.to_string();
    let mut captured_names: Vec<Ident> = Vec::new();
    let mut no_registry = false;
    let mut registry_name: Option<String> = None;

    for tok in attr_str.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if tok == "no_registry" || tok == "no_registry=true" || tok == "no_registry = true" {
            no_registry = true;
        } else if tok.starts_with("name=") || tok.starts_with("name =") {
            let name_part = tok.split('=').nth(1).unwrap_or("").trim();
            registry_name = Some(name_part.trim_matches('"').to_string());
        } else {
            captured_names.push(Ident::new(tok, Span::call_site()));
        }
    }

    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let attrs = &input.attrs;
    let fn_name = &sig.ident;

    // -- Resolve captured parameter types --------------------------------
    let param_map = param_type_map(sig);
    let captured_set: std::collections::HashSet<String> =
        captured_names.iter().map(|id| id.to_string()).collect();

    let mut field_types: Vec<syn::Type> = Vec::new();
    for name in &captured_names {
        match param_map.get(&name.to_string()) {
            Some(ty) => field_types.push(ty.clone()),
            None => {
                return syn::Error::new_spanned(
                    name,
                    format!("expected function parameter named `{}`", name),
                )
                .to_compile_error()
                .into();
            }
        }
    }

    // -- Generated names -------------------------------------------------
    let struct_name = format_ident!("JaminArgs_{}", fn_name);
    let impl_name = format_ident!("__jamin_impl_{}", fn_name);
    let wrapper_name = format_ident!("__jamin_registry_wrapper_{}", fn_name);
    let register_name = format_ident!("__register_jamin_{}", fn_name);
    let auto_register_name = format_ident!("__auto_register_jamin_{}", fn_name);

    // -- Deserialization struct -------------------------------------------
    let struct_def = quote! {
        #[allow(non_camel_case_types)]
        #[derive(Debug, serde::Deserialize, Clone)]
        struct #struct_name { #( pub #captured_names: #field_types ),* }
    };

    // -- Build new endpoint signature ------------------------------------
    // Keep non-captured params, append `mut payload: Multipart`, force OHS
    // return type.
    let mut non_captured_params: Vec<(Ident, syn::Type)> = Vec::new();
    let new_sig = {
        let mut s = sig.clone();
        s.inputs.clear();
        for arg in &sig.inputs {
            if let FnArg::Typed(pt) = arg
                && let Pat::Ident(pi) = &*pt.pat
            {
                if captured_set.contains(&pi.ident.to_string()) {
                    continue;
                }
                non_captured_params.push((pi.ident.clone(), (*pt.ty).clone()));
            }
            s.inputs.push(arg.clone());
        }
        // Remove captured params that were kept by the fallthrough above
        s.inputs = s
            .inputs
            .into_iter()
            .filter(|arg| {
                if let FnArg::Typed(pt) = arg
                    && let Pat::Ident(pi) = &*pt.pat
                {
                    return !captured_set.contains(&pi.ident.to_string());
                }
                true
            })
            .collect();

        let req_arg: FnArg = syn::parse_str("__jamin_req: actix_web::HttpRequest").unwrap();
        s.inputs.push(req_arg);
        let payload_arg: FnArg = syn::parse_str("mut payload: actix_multipart::Multipart").unwrap();
        s.inputs.push(payload_arg);
        s.output = syn::parse_str("-> Result<actix_web::HttpResponse, actix_web::Error>").unwrap();
        s
    };

    // -- Call-argument tokens (captured → `decoded.field.clone()`) --------
    let all_param_idents = collect_param_idents(sig);
    let call_args: proc_macro2::TokenStream = all_param_idents
        .iter()
        .map(|id| {
            if captured_set.contains(&id.to_string()) {
                quote! { decoded.#id.clone(), }
            } else {
                quote! { #id, }
            }
        })
        .collect();

    // -- Suppress registry if the impl returns HttpResponse --------------
    let impl_output = &sig.output;
    let impl_output_str = quote! { #impl_output }.to_string();
    if impl_output_str.contains("HttpResponse") {
        no_registry = true;
    }

    // -- Multipart decode tokens -----------------------------------------
    let decode_var = Ident::new("decoded", Span::call_site());
    let struct_ty = quote! { #struct_name };
    let decode_tokens = multipart_decode_tokens(&decode_var, &struct_ty);

    // -- OHS result tokens -----------------------------------------------
    let impl_call = quote! { #impl_name(#call_args).await };
    let ohs_tokens = ohs_result_tokens(&impl_call);

    // -- Shared impl + endpoint (always emitted) -------------------------
    let impl_inputs = &sig.inputs;
    let shared_and_endpoint = quote! {
        #struct_def

        #[allow(clippy::too_many_arguments)]
        async fn #impl_name(#impl_inputs) #impl_output {
            #block
        }

        #(#attrs)*
        #[allow(clippy::future_not_send)]
        #vis #new_sig {
            #decode_tokens
            #ohs_tokens
        }
    };

    // -- Registry wrapper (conditional) ----------------------------------
    let expanded = if !no_registry {
        let nc_idents: Vec<&Ident> = non_captured_params.iter().map(|(id, _)| id).collect();
        let nc_types: Vec<&syn::Type> = non_captured_params.iter().map(|(_, ty)| ty).collect();
        let reg_name = registry_name.unwrap_or_else(|| fn_name.to_string());

        // Build the closure body that downcasts MethodContext into the
        // concrete non-captured param types.
        //
        //  0 params → ctx ignored
        //  1 param  → downcast Box<dyn Any> to T directly
        //  N params → downcast Box<dyn Any> to (T1, T2, …) tuple, destructure
        let closure_body = if non_captured_params.is_empty() {
            quote! {
                Box::pin(async move {
                    #wrapper_name(project, data).await
                })
            }
        } else if non_captured_params.len() == 1 {
            let nc_id = &nc_idents[0];
            let nc_ty = &nc_types[0];
            quote! {
                Box::pin(async move {
                    let #nc_id: #nc_ty = *ctx
                        .downcast::<#nc_ty>()
                        .expect(concat!(
                            "registry context downcast failed for `",
                            stringify!(#nc_ty),
                            "` — caller passed wrong type",
                        ));
                    #wrapper_name(project, data, #nc_id).await
                })
            }
        } else {
            // N ≥ 2: pack as tuple (T1, T2, …)
            let tuple_ty = quote! { ( #( #nc_types ),* ) };
            quote! {
                Box::pin(async move {
                    let ( #( #nc_idents ),* ) = *ctx
                        .downcast::<#tuple_ty>()
                        .expect(concat!(
                            "registry context downcast failed for `",
                            stringify!(#tuple_ty),
                            "` — caller passed wrong type",
                        ));
                    #wrapper_name(project, data, #( #nc_idents ),*).await
                })
            }
        };

        // -- Wrapper extra params (the non-captured params, if any) ----------
        let wrapper_extra_params = if non_captured_params.is_empty() {
            quote! {}
        } else {
            quote! { #( #nc_idents: #nc_types, )* }
        };

        quote! {
            #shared_and_endpoint

            #[allow(clippy::future_not_send)]
            pub async fn #wrapper_name(
                project: String,
                data: serde_json::Value,
                #wrapper_extra_params
            ) -> Result<serde_json::Value, actix_web::Error> {
                let decoded: #struct_name = ::serde_json::from_value(data).map_err(|e| {
                    ::tracing::error!(error = %e, "failed to deserialize json into handler args");
                    ::actix_web::error::ErrorBadRequest(format!("deserialize error: {}", e))
                })?;

                let result = #impl_name(#call_args).await;
                match result {
                    Ok(v) => {
                        ::serde_json::to_value(&v).map_err(|e| {
                            ::actix_web::error::ErrorInternalServerError(
                                format!("serialize result: {}", e),
                            )
                        })
                    }
                    Err(e) => Err(e),
                }
            }

            pub fn #register_name() {
                jamin::registry::register_method(
                    #reg_name,
                    std::sync::Arc::new(|project, data, ctx| {
                        #closure_body
                    }),
                );
            }

            #[ctor::ctor]
            fn #auto_register_name() {
                #register_name();
            }
        }
    } else {
        shared_and_endpoint
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// #[jamin_batch]
// ---------------------------------------------------------------------------

/// Attribute macro for batch endpoints.
///
/// The attributed function's **first** parameter must be `Vec<T>` (the
/// deserialized batch items).  The macro replaces it with multipart
/// extraction + jamin decoding, then jamin-encodes the `Result` return
/// value and wraps it in `<ohs>…</ohs>`.
#[proc_macro_attribute]
pub fn jamin_batch(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let attrs = &input.attrs;

    // -- First parameter must be `name: Vec<T>` --------------------------
    let first_param = match sig.inputs.first() {
        Some(FnArg::Typed(p)) => p,
        _ => {
            return syn::Error::new_spanned(
                &sig.ident,
                "expected first parameter to be `requests: Vec<T>`",
            )
            .to_compile_error()
            .into();
        }
    };

    let first_ident = if let Pat::Ident(p) = &*first_param.pat {
        p.ident.clone()
    } else {
        return syn::Error::new_spanned(&first_param.pat, "expected ident pattern")
            .to_compile_error()
            .into();
    };

    let first_ty = &first_param.ty;

    // -- Build new signature (drop first param, add payload) -------------
    let new_sig = {
        let mut s = sig.clone();
        s.inputs.clear();
        for (i, arg) in sig.inputs.iter().enumerate() {
            if i > 0 {
                s.inputs.push(arg.clone());
            }
        }
        let req_arg: FnArg = syn::parse_str("__jamin_req: actix_web::HttpRequest").unwrap();
        s.inputs.push(req_arg);
        let payload_arg: FnArg = syn::parse_str("mut payload: actix_multipart::Multipart").unwrap();
        s.inputs.push(payload_arg);
        s.output = syn::parse_str("-> Result<actix_web::HttpResponse, actix_web::Error>").unwrap();
        s
    };

    // -- Tokens ----------------------------------------------------------
    let target_ty = quote! { #first_ty };
    let decode_tokens = multipart_decode_tokens(&first_ident, &target_ty);
    let result_expr = quote! { (async move { #block }).await };
    let ohs_tokens = ohs_result_tokens(&result_expr);

    let expanded = quote! {
        #(#attrs)*
        #[allow(clippy::future_not_send)]
        #vis #new_sig {
            #decode_tokens
            #ohs_tokens
        }
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// #[ohs_response]
// ---------------------------------------------------------------------------

/// Wraps a handler so its `Result<T: Serialize, Error>` return value is
/// jamin-encoded and wrapped in `<ohs>…</ohs>`.  Does **not** touch the
/// parameters (no multipart extraction).
#[proc_macro_attribute]
pub fn ohs_response(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let sig = &input.sig;
    let block = &input.block;
    let attrs = &input.attrs;
    let fn_name = &sig.ident;

    let impl_name = format_ident!("__ohs_impl_{}", fn_name);
    let impl_inputs = &sig.inputs;
    let impl_output = &sig.output;
    let arg_idents = collect_param_idents(sig);

    let mut new_sig = sig.clone();
    new_sig.output =
        syn::parse_str("-> Result<actix_web::HttpResponse, actix_web::Error>").unwrap();

    let result_expr = quote! { #impl_name(#(#arg_idents),*).await };
    let ohs_tokens = ohs_result_tokens(&result_expr);

    let expanded = quote! {
        async fn #impl_name(#impl_inputs) #impl_output {
            #block
        }

        #(#attrs)*
        #[allow(clippy::future_not_send)]
        #new_sig {
            #ohs_tokens
        }
    };

    expanded.into()
}
