#![deny(dead_code)]
#![deny(missing_docs)]
#![deny(unused)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

//! Derive crate for `prometheus_client`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

fn error_spanned<T: quote::ToTokens>(tokens: &T, msg: &str) -> TokenStream {
    syn::Error::new_spanned(tokens, msg)
        .to_compile_error()
        .into()
}

/// Derive `prometheus_client::encoding::EncodeLabelSet`.
#[proc_macro_derive(EncodeLabelSet, attributes(prometheus))]
pub fn derive_encode_label_set(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    let body: TokenStream2 = match ast.clone().data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(syn::FieldsNamed { named, .. }) => {
                let body = named
                    .into_iter()
                    .map(|f| {
                        let ident = f.ident.unwrap();
                        let flatten = match f
                            .attrs
                            .iter()
                            .find(|a| a.path().is_ident("prometheus"))
                        {
                            None => false,
                            Some(a) => match a.parse_args::<syn::Ident>() {
                                Ok(ident) if ident == "flatten" => true,
                                Ok(other) => {
                                    return Err(error_spanned(
                                        &other,
                                        &format!(
                                            "Provided attribute '{other}', but only 'flatten' is supported"
                                        ),
                                    ));
                                }
                                Err(_) => {
                                    return Err(error_spanned(
                                        a,
                                        "Attribute on `#[prometheus(...)]` must be an identifier, e.g. `#[prometheus(flatten)]`",
                                    ));
                                }
                            },
                        };

                        if flatten {
                            Ok(quote! {
                                 EncodeLabelSet::encode(&self.#ident, encoder)?;
                            })
                        } else {
                            let ident_string = KEYWORD_IDENTIFIERS
                                .iter()
                                .find(|pair| ident == pair.1)
                                .map(|pair| pair.0.to_string())
                                .unwrap_or_else(|| ident.to_string());

                            Ok(quote! {
                                let mut label_encoder = encoder.encode_label();
                                let mut label_key_encoder = label_encoder.encode_label_key()?;
                                EncodeLabelKey::encode(&#ident_string, &mut label_key_encoder)?;

                                let mut label_value_encoder = label_key_encoder.encode_label_value()?;
                                EncodeLabelValue::encode(&self.#ident, &mut label_value_encoder)?;

                                label_value_encoder.finish()?;
                            })
                        }
                    })
                    .collect::<Result<TokenStream2, TokenStream>>();

                match body {
                    Ok(body) => body,
                    Err(err) => return err,
                }
            }
            syn::Fields::Unnamed(_) => {
                return error_spanned(
                    &ast,
                    "Can not derive Encode for struct with unnamed fields.",
                );
            }
            syn::Fields::Unit => {
                return error_spanned(&ast, "Can not derive Encode for struct with unit field.");
            }
        },
        syn::Data::Enum(syn::DataEnum { .. }) => {
            return error_spanned(&ast, "Can not derive Encode for enum.");
        }
        syn::Data::Union(_) => {
            return error_spanned(&ast, "Can not derive Encode for union.");
        }
    };

    let gen = quote! {
        impl ::prometheus_client::encoding::EncodeLabelSet for #name {
            fn encode(&self, encoder: &mut ::prometheus_client::encoding::LabelSetEncoder) -> ::core::result::Result<(), ::core::fmt::Error> {
                use ::prometheus_client::encoding::EncodeLabel;
                use ::prometheus_client::encoding::EncodeLabelKey;
                use ::prometheus_client::encoding::EncodeLabelValue;

                #body

                ::core::result::Result::Ok(())
            }
        }
    };

    gen.into()
}

/// Derive `prometheus_client::encoding::EncodeLabelValue`.
#[proc_macro_derive(EncodeLabelValue)]
pub fn derive_encode_label_value(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    let body = match ast.clone().data {
        syn::Data::Struct(_) => {
            return error_spanned(&ast, "Can not derive EncodeLabel for struct.");
        }
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            let match_arms: TokenStream2 = variants
                .into_iter()
                .map(|v| {
                    let ident = v.ident;
                    quote! {
                        #name::#ident => encoder.write_str(stringify!(#ident))?,
                    }
                })
                .collect();

            quote! {
                match self {
                    #match_arms
                }
            }
        }
        syn::Data::Union(_) => {
            return error_spanned(&ast, "Can not derive Encode for union.");
        }
    };

    let gen = quote! {
        impl ::prometheus_client::encoding::EncodeLabelValue for #name {
            fn encode(&self, encoder: &mut ::prometheus_client::encoding::LabelValueEncoder) -> ::core::result::Result<(), ::core::fmt::Error> {
                use ::core::fmt::Write;

                #body

                ::core::result::Result::Ok(())
            }
        }
    };

    gen.into()
}

// Copied from https://github.com/djc/askama (MIT and APACHE licensed) and
// modified.
static KEYWORD_IDENTIFIERS: [(&str, &str); 48] = [
    ("as", "r#as"),
    ("break", "r#break"),
    ("const", "r#const"),
    ("continue", "r#continue"),
    ("crate", "r#crate"),
    ("else", "r#else"),
    ("enum", "r#enum"),
    ("extern", "r#extern"),
    ("false", "r#false"),
    ("fn", "r#fn"),
    ("for", "r#for"),
    ("if", "r#if"),
    ("impl", "r#impl"),
    ("in", "r#in"),
    ("let", "r#let"),
    ("loop", "r#loop"),
    ("match", "r#match"),
    ("mod", "r#mod"),
    ("move", "r#move"),
    ("mut", "r#mut"),
    ("pub", "r#pub"),
    ("ref", "r#ref"),
    ("return", "r#return"),
    ("static", "r#static"),
    ("struct", "r#struct"),
    ("trait", "r#trait"),
    ("true", "r#true"),
    ("type", "r#type"),
    ("unsafe", "r#unsafe"),
    ("use", "r#use"),
    ("where", "r#where"),
    ("while", "r#while"),
    ("async", "r#async"),
    ("await", "r#await"),
    ("dyn", "r#dyn"),
    ("abstract", "r#abstract"),
    ("become", "r#become"),
    ("box", "r#box"),
    ("do", "r#do"),
    ("final", "r#final"),
    ("macro", "r#macro"),
    ("override", "r#override"),
    ("priv", "r#priv"),
    ("typeof", "r#typeof"),
    ("unsized", "r#unsized"),
    ("virtual", "r#virtual"),
    ("yield", "r#yield"),
    ("try", "r#try"),
];
