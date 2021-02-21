extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::export::TokenStream2;
use syn::DeriveInput;

#[proc_macro_derive(Encode)]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    let body: TokenStream2 = match ast.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(syn::FieldsNamed { named, .. }) => named
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    let ident = f.ident.unwrap();
                    let maybe_comma = if i == 0 {
                        TokenStream2::default()
                    } else {
                        quote! { writer.write_all(b",")?; }
                    };
                    quote! {
                        #maybe_comma
                        writer.write_all(concat!(stringify!(#ident), "=\"").as_bytes())?;
                        open_metrics_client::encoding::text::Encode::encode(&self.#ident, writer)?;
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
        impl open_metrics_client::encoding::text::Encode for #name {
            fn encode(&self, writer: &mut dyn std::io::Write) -> std::result::Result<(), std::io::Error> {
                #body

                Ok(())
            }
        }
    };
    gen.into()
}
