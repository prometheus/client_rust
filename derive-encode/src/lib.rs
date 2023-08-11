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

/// Derive `prometheus_client::encoding::EncodeLabelSet`.
#[proc_macro_derive(EncodeLabelSet, attributes(prometheus))]
pub fn derive_encode_label_set(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    let body: TokenStream2 = match ast.clone().data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(syn::FieldsNamed { named, .. }) => named
                .into_iter()
                .map(|f| {
                    let attribute = f
                        .attrs
                        .iter()
                        .find(|a| a.path().is_ident("prometheus"))
                        .map(|a| a.parse_args::<syn::Ident>().unwrap().to_string());
                    let flatten = match attribute.as_deref() {
                        Some("flatten") => true,
                        Some(other) => {
                            panic!("Provided attribute '{other}', but only 'flatten' is supported")
                        }
                        None => false,
                    };
                    let ident = f.ident.unwrap();
                    if flatten {
                        quote! {
                             EncodeLabelSet::encode(&self.#ident, encoder)?;
                        }
                    } else {
                        let ident_string = KEYWORD_IDENTIFIERS
                            .iter()
                            .find(|pair| ident == pair.1)
                            .map(|pair| pair.0.to_string())
                            .unwrap_or_else(|| ident.to_string());

                        quote! {
                            let mut label_encoder = encoder.encode_label();
                            let mut label_key_encoder = label_encoder.encode_label_key()?;
                            EncodeLabelKey::encode(&#ident_string, &mut label_key_encoder)?;

                            let mut label_value_encoder = label_key_encoder.encode_label_value()?;
                            EncodeLabelValue::encode(&self.#ident, &mut label_value_encoder)?;

                            label_value_encoder.finish()?;
                        }
                    }
                })
                .collect(),
            syn::Fields::Unnamed(_) => {
                panic!("Can not derive Encode for struct with unnamed fields.")
            }
            syn::Fields::Unit => panic!("Can not derive Encode for struct with unit field."),
        },
        syn::Data::Enum(syn::DataEnum { .. }) => {
            panic!("Can not derive Encode for enum.")
        }
        syn::Data::Union(_) => panic!("Can not derive Encode for union."),
    };

    let gen = quote! {
        impl prometheus_client::encoding::EncodeLabelSet for #name {
            fn encode(&self, mut encoder: prometheus_client::encoding::LabelSetEncoder) -> std::result::Result<(), std::fmt::Error> {
                use prometheus_client::encoding::EncodeLabel;
                use prometheus_client::encoding::EncodeLabelKey;
                use prometheus_client::encoding::EncodeLabelValue;

                #body

                Ok(())
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
            panic!("Can not derive EncodeLabel for struct.")
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
        syn::Data::Union(_) => panic!("Can not derive Encode for union."),
    };

    let gen = quote! {
        impl prometheus_client::encoding::EncodeLabelValue for #name {
            fn encode(&self, encoder: &mut prometheus_client::encoding::LabelValueEncoder) -> std::result::Result<(), std::fmt::Error> {
                use std::fmt::Write;

                #body

                Ok(())
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
