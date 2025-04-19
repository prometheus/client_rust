use quote::ToTokens;
use crate::registrant::attribute;
use super::attribute::Attribute;

// do not derive debug since this needs "extra-traits"
// feature for crate `syn`, which slows compile time
// too much, and is not needed as this struct is not
// public.
pub struct Field {
    ident: syn::Ident,
    name: syn::LitStr,
    attr: Attribute,
}

impl Field {
    pub(super) fn ident(&self) -> &syn::Ident {
        &self.ident
    }

    pub(super) fn name(&self) -> &syn::LitStr {
        match &self.attr.rename {
            Some(rename) => rename,
            None => &self.name,
        }
    }

    pub(super) fn help(&self) -> syn::LitStr {
        self.attr.help.clone()
            .unwrap_or_else(|| {
                syn::LitStr::new(
                    "",
                    self.ident.span(),
                )
            })
    }

    pub(super) fn unit(&self) -> Option<&syn::LitStr> {
        self.attr.unit.as_ref()
    }

    pub(super) fn skip(&self) -> bool {
        self.attr.skip
    }
}

impl TryFrom<syn::Field> for Field {
    type Error = syn::Error;

    fn try_from(field: syn::Field) -> Result<Self, Self::Error> {
        let ident = field.ident.clone().expect("Fields::Named should have an identifier");
        let name = syn::LitStr::new(
            &ident.to_string(),
            ident.span(),
        );
        let attr = field
            .attrs
            .into_iter()
            // ignore unknown attributes, which might be defined by another derive macros.
            .filter(|attr| attr.path().is_ident("doc") || attr.path().is_ident("registrant") )
            .try_fold(vec![], |mut acc, attr| {
                acc.push(syn::parse2::<Attribute>(attr.meta.into_token_stream())?);
                Ok::<Vec<attribute::Attribute>, syn::Error>(acc)
            })?
            .into_iter()
            .try_fold(Attribute::default(), |acc, attr| {
                acc.merge(attr)
            })?;
        Ok(Field{
            ident,
            name,
            attr,
        })
    }
}
