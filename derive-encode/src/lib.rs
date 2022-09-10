extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

#[proc_macro_derive(Encode)]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    let body = match ast.clone().data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(syn::FieldsNamed { named, .. }) => named
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    let ident = f.ident.unwrap();
                    let ident_string = KEYWORD_IDENTIFIERS
                        .iter()
                        .find(|pair| ident == pair.1)
                        .map(|pair| pair.0.to_string())
                        .unwrap_or_else(|| ident.to_string());

                    let maybe_comma = if i == 0 {
                        TokenStream2::default()
                    } else {
                        quote! { writer.write_all(b",")?; }
                    };
                    quote! {
                        #maybe_comma
                        writer.write_all(concat!(#ident_string, "=\"").as_bytes())?;
                        prometheus_client::encoding::text::Encode::encode(&self.#ident, writer)?;
                        writer.write_all(b"\"")?;
                    }
                })
                .collect(),
            syn::Fields::Unnamed(_) => {
                panic!("Can not derive Encode for struct with unnamed fields.")
            }
            syn::Fields::Unit => panic!("Can not derive Encode for struct with unit field."),
        },
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            let match_arms: TokenStream2 = variants
                .into_iter()
                .map(|v| {
                    let ident = v.ident;
                    quote! {
                        #name::#ident => writer.write_all(stringify!(#ident).as_bytes())?,
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
        impl prometheus_client::encoding::text::Encode for #name {
            fn encode(&self, writer: &mut dyn std::io::Write) -> std::result::Result<(), std::io::Error> {
                #body

                Ok(())
            }
        }
    };

    #[cfg(feature = "protobuf")]
    let gen = {
        let protobuf = derive_protobuf_encode(ast);
        quote! {
            #gen

            #protobuf
        }
    };

    gen.into()
}

#[cfg(feature = "protobuf")]
fn derive_protobuf_encode(ast: DeriveInput) -> TokenStream2 {
    let name = &ast.ident;

    match ast.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(syn::FieldsNamed { named, .. }) => {
                let push_labels: TokenStream2 = named
                    .into_iter()
                    .map(|f| {
                        let ident = f.ident.unwrap();
                        let ident_string = KEYWORD_IDENTIFIERS
                            .iter()
                            .find(|pair| ident == pair.1)
                            .map(|pair| pair.0.to_string())
                            .unwrap_or_else(|| ident.to_string());

                        quote! {
                            let mut label = {
                                let mut labels = vec![];
                                self.#ident.encode(&mut labels);
                                debug_assert_eq!(1, labels.len(), "Labels encoded from {} should have only one label.", #ident_string);
                                labels.pop().expect("should have an element")
                            };
                            // Override the label name with the field name of this struct.
                            label.name = #ident_string.to_string();
                            labels.push(label);
                        }
                    })
                    .collect();

                quote! {
                    impl prometheus_client::encoding::proto::EncodeLabels for #name {
                        fn encode(&self, labels: &mut Vec<prometheus_client::encoding::proto::Label>) {
                            #push_labels
                        }
                    }
                }
            }
            syn::Fields::Unnamed(_) => {
                panic!("Can not derive Encode for struct with unnamed fields.")
            }
            syn::Fields::Unit => panic!("Can not derive Encode for struct with unit field."),
        },
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            let match_arms: TokenStream2 = variants
                .into_iter()
                .map(|v| {
                    let ident = v.ident;
                    quote! {
                        #name::#ident => {
                            let mut label = prometheus_client::encoding::proto::Label::default();
                            label.name = stringify!(#name).to_string();
                            label.value = stringify!(#ident).to_string();
                            labels.push(label);
                        }
                    }
                })
                .collect();

            quote! {
                impl prometheus_client::encoding::proto::EncodeLabels for #name {
                    fn encode(&self, labels: &mut Vec<prometheus_client::encoding::proto::Label>) {
                        match self {
                            #match_arms
                        };
                    }
                }
            }
        }
        syn::Data::Union(_) => panic!("Can not derive Encode for union."),
    }
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
