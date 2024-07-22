use darling::{ast, util::Flag, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Expr, Generics, Ident, Lit, Meta, Type};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(register), supports(struct_named))]
struct Register {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), RegisterField>,
}

#[derive(Debug, FromField)]
#[darling(attributes(register), forward_attrs(doc))]
struct RegisterField {
    ident: Option<Ident>,
    ty: Type,
    attrs: Vec<syn::Attribute>,
    skip: Flag,
    unit: Option<String>,
}

#[proc_macro_derive(Register, attributes(register))]
pub fn derive_register(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let info = Register::from_derive_input(&ast).unwrap();

    let name = info.ident;
    let (impl_generics, ty_generics, where_clause) = info.generics.split_for_impl();

    let field_register = info
        .data
        .take_struct()
        .unwrap()
        .into_iter()
        .filter(|x| !x.skip.is_present())
        .map(|field| {
            let mut help = String::new();
            for attr in field.attrs {
                let path = attr.path();
                if path.is_ident("doc") && help.is_empty() {
                    if let Some(doc) = extract_doc_comment(&attr.meta) {
                        help = doc.trim().to_string();
                    }
                }
            }

            let ident = field.ident.unwrap();
            let ty = field.ty;
            let name = ident.to_string();

            let unit = if let Some(unit) = field.unit {
                quote!(Some(::prometheus_client::registry::Unit::Other(#unit.to_string())))
            } else {
                quote!(None)
            };

            quote! {
                <#ty as ::prometheus_client::registry::RegisterField>::register_field(
                    &self.#ident,
                    #name,
                    #help,
                    #unit,
                    registry,
                )
            }
        });

    quote! {
        impl #impl_generics ::prometheus_client::registry::Register for #name #ty_generics #where_clause {
            fn register(&self, registry: &mut ::prometheus_client::registry::Registry) {
                #(#field_register);*
            }
        }

        impl #impl_generics ::prometheus_client::registry::RegisterField for #name #ty_generics #where_clause {
            fn register_field<N: ::std::convert::Into<::std::string::String>, H: ::std::convert::Into<::std::string::String>>(
                &self,
                name: N,
                help: H,
                unit: Option<::prometheus_client::registry::Unit>,
                registry: &mut ::prometheus_client::registry::Registry)
            {
                let name = name.into();
                let mut registry = registry.sub_registry_with_prefix(name);
                <Self as ::prometheus_client::registry::Register>::register(&self, &mut registry);
            }
        }
    }.into()
}

fn extract_doc_comment(meta: &Meta) -> Option<String> {
    let Meta::NameValue(nv) = meta else {
        return None;
    };
    let Expr::Lit(lit) = &nv.value else {
        return None;
    };
    let Lit::Str(lit_str) = &lit.lit else {
        return None;
    };
    Some(lit_str.value())
}
