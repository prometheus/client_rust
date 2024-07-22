use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Expr, Fields, Lit, Meta};

#[proc_macro_derive(Register)]
pub fn derive_register(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    let name = ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let Data::Struct(strukt) = ast.data else {
        panic!("Register can only be derived on a struct.");
    };

    let Fields::Named(fields) = strukt.fields else {
        panic!("Register can only be derived on a struct with named fields.");
    };

    let field_register = fields.named.into_iter().map(|field| {
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

        quote! {
            <#ty as ::prometheus_client::registry::RegisterField>::register_field(
                self.#ident,
                #name,
                #help,
                None,
                registry,
            )
        }
    });

    quote! {
        impl #impl_generics ::prometheus_client::registry::Register for #name #ty_generics #where_clause {
            fn register(self, registry: &mut ::prometheus_client::registry::Registry) {
                <Self as ::prometheus_client::registry::RegisterField>::register_field(self, "", "", None, registry);
            }
        }

        impl #impl_generics ::prometheus_client::registry::RegisterField for #name #ty_generics #where_clause {
            fn register_field<N: Into<String>, H: Into<String>>(
                self,
                name: N,
                help: H,
                unit: Option<::prometheus_client::registry::Unit>,
                registry: &mut ::prometheus_client::registry::Registry)
            {
                #(#field_register);*
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
