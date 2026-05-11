// Derive macros for Opake.
//
// RedactedDebug generates three impls for structs with `#[redact]` fields:
// 1. Debug — shows byte length instead of content for redacted fields
// 2. Zeroize — overwrites redacted fields with zeros
// 3. Drop — calls zeroize() automatically
//
// If a field is sensitive enough to redact from debug output, it's sensitive
// enough to zeroize on drop. No secret material should linger in memory.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, FnArg, ItemFn, Pat};

/// Auto-persist the session after an async FileManager method.
///
/// Transforms an async method that returns `Result<T, Error>` into a
/// wrapper + inner pair. The wrapper calls the inner method, then
/// `self.opake.signoff(result).await` to persist the session if it was
/// refreshed during the call.
///
/// # Before
/// ```ignore
/// #[signoff]
/// pub async fn upload(&mut self, req: &UploadRequest<'_>) -> Result<UploadResult, Error> {
///     // ... method body
/// }
/// ```
///
/// # After (generated)
/// ```ignore
/// pub async fn upload(&mut self, req: &UploadRequest<'_>) -> Result<UploadResult, Error> {
///     let result = self.__upload_inner(req).await;
///     self.opake.signoff(result).await
/// }
/// async fn __upload_inner(&mut self, req: &UploadRequest<'_>) -> Result<UploadResult, Error> {
///     // ... original method body
/// }
/// ```
#[proc_macro_attribute]
pub fn signoff(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);

    if func.sig.asyncness.is_none() {
        return syn::Error::new_spanned(&func.sig, "#[signoff] requires an async fn")
            .to_compile_error()
            .into();
    }

    let vis = &func.vis;
    let sig = &func.sig;
    let inner_name = format_ident!("__{}_inner", sig.ident, span = Span::call_site());
    let body = &func.block;
    let attrs: Vec<_> = func.attrs.iter().collect();

    // Extract argument identifiers to forward to the inner method
    let forwarded_args: Vec<_> = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => {
                let pat = &pat_type.pat;
                // Strip reference patterns: &x → x, &mut x → x
                let ident = strip_ref_pat(pat);
                Some(ident)
            }
        })
        .collect();

    // Build the inner function signature (same params, private)
    let mut inner_sig = sig.clone();
    inner_sig.ident = inner_name.clone();

    // Parse optional attribute argument: #[signoff] vs #[signoff(self)]
    // Default (no arg): FileManager pattern → self.opake.signoff()
    // With (self): Opake pattern → self.signoff()
    let attr_tokens: proc_macro2::TokenStream = _attr.into();
    let use_self_signoff = !attr_tokens.is_empty();

    let signoff_call = if use_self_signoff {
        quote! { self.signoff(__result).await }
    } else {
        quote! { self.opake.signoff(__result).await }
    };

    let expanded = quote! {
        #(#attrs)*
        #vis #sig {
            let __result = self.#inner_name(#(#forwarded_args),*).await;
            #signoff_call
        }

        #inner_sig #body
    };

    expanded.into()
}

/// Strip `&` / `&mut` from a pattern to get the inner identifier for forwarding.
fn strip_ref_pat(pat: &Pat) -> proc_macro2::TokenStream {
    match pat {
        Pat::Ident(pi) => {
            let ident = &pi.ident;
            quote! { #ident }
        }
        Pat::Reference(pr) => strip_ref_pat(&pr.pat),
        other => quote! { #other },
    }
}

fn is_redacted(f: &syn::Field) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("redact"))
}

/// Derive `Debug`, `Zeroize`, and `Drop` for structs with `#[redact]` fields.
///
/// Redacted fields are shown as `[N bytes]` in debug output and zeroized on
/// drop. Non-redacted fields are printed normally and left untouched.
///
/// # Named structs
///
/// ```ignore
/// #[derive(RedactedDebug)]
/// pub struct Session {
///     pub did: String,
///     pub handle: String,
///     #[redact] pub access_jwt: String,
///     #[redact] pub refresh_jwt: String,
/// }
/// // Debug: Session { did: "...", handle: "...", access_jwt: [187 bytes], refresh_jwt: [253 bytes] }
/// // Drop: access_jwt and refresh_jwt zeroed; did and handle untouched
/// ```
///
/// # Tuple structs (newtypes)
///
/// ```ignore
/// #[derive(RedactedDebug)]
/// pub struct ContentKey(#[redact] pub [u8; 32]);
/// // Debug: ContentKey([32 bytes])
/// // Drop: inner bytes zeroed
/// ```
#[proc_macro_derive(RedactedDebug, attributes(redact))]
pub fn redacted_debug_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let (debug_body, zeroize_body, has_redacted) = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let debug_stmts: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| {
                        let field_name = f.ident.as_ref().unwrap();
                        if is_redacted(f) {
                            quote! {
                                s.field(
                                    stringify!(#field_name),
                                    &::opake_crypto::Redacted(&self.#field_name),
                                );
                            }
                        } else {
                            quote! {
                                s.field(stringify!(#field_name), &self.#field_name);
                            }
                        }
                    })
                    .collect();

                let zeroize_stmts: Vec<_> = fields
                    .named
                    .iter()
                    .filter(|f| is_redacted(f))
                    .map(|f| {
                        let field_name = f.ident.as_ref().unwrap();
                        quote! { self.#field_name.zeroize(); }
                    })
                    .collect();

                let has_redacted = fields.named.iter().any(is_redacted);

                let debug = quote! {
                    let mut s = f.debug_struct(stringify!(#name));
                    #(#debug_stmts)*
                    s.finish()
                };

                let zeroize = quote! { #(#zeroize_stmts)* };

                (debug, zeroize, has_redacted)
            }
            Fields::Unnamed(fields) => {
                let debug_stmts: Vec<_> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let index = syn::Index::from(i);
                        if is_redacted(f) {
                            quote! {
                                s.field(&::opake_crypto::Redacted(&self.#index));
                            }
                        } else {
                            quote! {
                                s.field(&self.#index);
                            }
                        }
                    })
                    .collect();

                let zeroize_stmts: Vec<_> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| is_redacted(f))
                    .map(|(i, _)| {
                        let index = syn::Index::from(i);
                        quote! { self.#index.zeroize(); }
                    })
                    .collect();

                let has_redacted = fields.unnamed.iter().any(is_redacted);

                let debug = quote! {
                    let mut s = f.debug_tuple(stringify!(#name));
                    #(#debug_stmts)*
                    s.finish()
                };

                let zeroize = quote! { #(#zeroize_stmts)* };

                (debug, zeroize, has_redacted)
            }
            Fields::Unit => (quote! { f.write_str(stringify!(#name)) }, quote! {}, false),
        },
        _ => {
            return syn::Error::new_spanned(&input, "RedactedDebug only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let debug_impl = quote! {
        impl ::std::fmt::Debug for #name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                #debug_body
            }
        }
    };

    // Only generate Zeroize + Drop if the struct has at least one #[redact] field.
    // Structs without redacted fields get Debug only (no unnecessary Drop impl).
    let zeroize_impl = if has_redacted {
        quote! {
            impl ::zeroize::Zeroize for #name {
                fn zeroize(&mut self) {
                    use ::zeroize::Zeroize;
                    #zeroize_body
                }
            }

            impl ::std::ops::Drop for #name {
                fn drop(&mut self) {
                    use ::zeroize::Zeroize;
                    self.zeroize();
                }
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #debug_impl
        #zeroize_impl
    };

    expanded.into()
}
