mod attribute;
mod field;

use crate::{Error, Result, TokenStream2};
use quote::quote;

pub fn registrant_impl(input: TokenStream2) -> Result<TokenStream2> {
    let ast = syn::parse2::<syn::DeriveInput>(input)?;
    let name = ast.ident;
    let fields = match ast.data {
        syn::Data::Struct(body) => match body.fields {
            syn::Fields::Named(fields) => fields,
            syn::Fields::Unnamed(fields) => {
                return Err(Error::new_spanned(
                    fields,
                    "Can not derive Registrant for struct with unnamed fields.",
                ));
            }
            syn::Fields::Unit => {
                return Err(Error::new_spanned(
                    name,
                    "Can not derive Registrant for unit struct.",
                ));
            }
        },
        syn::Data::Enum(_) => {
            return Err(Error::new_spanned(
                name,
                "Can not derive Registrant for enum.",
            ));
        }
        syn::Data::Union(_) => {
            return Err(Error::new_spanned(
                name,
                "Can not derive Registrant for union.",
            ));
        }
    };

    let register_calls = fields
        .named
        .into_iter()
        .try_fold(vec![], |mut acc, field| {
            acc.push(field::Field::try_from(field)?);
            Ok::<Vec<field::Field>, syn::Error>(acc)
        })?
        .into_iter()
        .filter_map(|field| {
            if field.skip() {
                return None;
            }

            let ident = field.ident();
            let name = field.name();
            let help = field.help();
            let body = match field.unit() {
                Some(unit) => {
                    quote! {
                        registry.register_with_unit(
                            #name,
                            #help,
                            ::prometheus_client::registry::Unit::Other(#unit.to_string()),
                            self.#ident.clone(),
                        );
                    }
                }
                None => {
                    quote! {
                        registry.register(
                            #name,
                            #help,
                            self.#ident.clone(),
                        );
                    }
                }
            };

            Some(body)
        });

    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::prometheus_client::registry::Registrant for #name #ty_generics #where_clause {
            fn register(&self, registry: &mut ::prometheus_client::registry::Registry) {
                #(#register_calls)*
            }
        }
    })
}
