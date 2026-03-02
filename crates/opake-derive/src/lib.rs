// Derive macros for Opake.
//
// RedactedDebug generates a Debug impl that shows byte length instead of
// content for fields marked `#[redact]`. Works on named structs and
// newtypes. Redacted fields display as `[N bytes]` for sized types and
// `Some([N bytes])` / `None` for Options.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Derive a `Debug` impl that redacts fields marked with `#[redact]`.
///
/// Redacted fields show their byte length instead of content, using
/// `opake_core::crypto::Redacted` as the formatting wrapper.
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
/// // Debug output: Session { did: "...", handle: "...", access_jwt: [187 bytes], refresh_jwt: [253 bytes] }
/// ```
///
/// # Tuple structs (newtypes)
///
/// ```ignore
/// #[derive(RedactedDebug)]
/// pub struct ContentKey(#[redact] pub [u8; 32]);
/// // Debug output: ContentKey([32 bytes])
/// ```
#[proc_macro_derive(RedactedDebug, attributes(redact))]
pub fn redacted_debug_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let body = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let field_stmts: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| {
                        let field_name = f.ident.as_ref().unwrap();
                        let is_redacted = f.attrs.iter().any(|a| a.path().is_ident("redact"));
                        if is_redacted {
                            quote! {
                                s.field(
                                    stringify!(#field_name),
                                    &::opake_core::crypto::Redacted(&self.#field_name),
                                );
                            }
                        } else {
                            quote! {
                                s.field(stringify!(#field_name), &self.#field_name);
                            }
                        }
                    })
                    .collect();

                quote! {
                    let mut s = f.debug_struct(stringify!(#name));
                    #(#field_stmts)*
                    s.finish()
                }
            }
            Fields::Unnamed(fields) => {
                let field_stmts: Vec<_> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let index = syn::Index::from(i);
                        let is_redacted = f.attrs.iter().any(|a| a.path().is_ident("redact"));
                        if is_redacted {
                            quote! {
                                s.field(&::opake_core::crypto::Redacted(&self.#index));
                            }
                        } else {
                            quote! {
                                s.field(&self.#index);
                            }
                        }
                    })
                    .collect();

                quote! {
                    let mut s = f.debug_tuple(stringify!(#name));
                    #(#field_stmts)*
                    s.finish()
                }
            }
            Fields::Unit => {
                quote! { f.write_str(stringify!(#name)) }
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "RedactedDebug only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let expanded = quote! {
        impl ::std::fmt::Debug for #name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                #body
            }
        }
    };

    expanded.into()
}
