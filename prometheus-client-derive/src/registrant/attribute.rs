use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;

// do not derive debug since this needs "extra-traits"
// feature for crate `syn`, which slows compile time
// too much, and is not needed as this struct is not
// public.
#[derive(Default)]
pub struct Attribute {
    pub help: Option<syn::LitStr>,
    pub unit: Option<syn::LitStr>,
    pub rename: Option<syn::LitStr>,
    pub skip: bool,
}

impl Attribute {
    fn with_help(mut self, doc: syn::LitStr) -> Self {
        self.help = Some(doc);
        self
    }

    pub(super) fn merge(self, other: Self) -> syn::Result<Self> {
        let mut merged = self;

        if let Some(help) = other.help {
            // trim leading and trailing whitespace
            // and add a space between the two doc strings
            let mut acc = merged
                .help
                .unwrap_or_else(|| syn::LitStr::new("", help.span()))
                .value()
                .trim()
                .to_string();
            acc.push(' ');
            acc.push_str(help.value().trim());
            merged.help = Some(syn::LitStr::new(&acc, Span::call_site()));
        }
        if let Some(unit) = other.unit {
            if merged.unit.is_some() {
                return Err(syn::Error::new_spanned(
                    merged.unit,
                    "Duplicate `unit` attribute",
                ));
            }

            merged.unit = Some(unit);
        }
        if let Some(rename) = other.rename {
            if merged.rename.is_some() {
                return Err(syn::Error::new_spanned(
                    merged.rename,
                    "Duplicate `rename` attribute",
                ));
            }

            merged.rename = Some(rename);
        }
        if other.skip {
            merged.skip = merged.skip || other.skip;
        }

        Ok(merged)
    }
}

impl syn::parse::Parse for Attribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let meta = input.parse::<syn::Meta>()?;
        let span = meta.span();

        match meta {
            syn::Meta::NameValue(meta) if meta.path.is_ident("doc") => {
                if let syn::Expr::Lit(lit) = meta.value {
                    let lit_str = syn::parse2::<syn::LitStr>(lit.lit.to_token_stream())?;
                    Ok(Attribute::default().with_help(lit_str))
                } else {
                    Err(syn::Error::new_spanned(
                        meta.value,
                        "Expected a string literal for doc attribute",
                    ))
                }
            }
            syn::Meta::List(meta) if meta.path.is_ident("registrant") => {
                let mut attr = Attribute::default();
                meta.parse_nested_meta(|meta| {
                    if meta.path.is_ident("unit") {
                        let unit = meta.value()?.parse::<syn::LitStr>()?;

                        if attr.unit.is_some() {
                            return Err(syn::Error::new(
                                meta.path.span(),
                                "Duplicate `unit` attribute",
                            ));
                        }

                        // unit should be lowercase
                        let unit = syn::LitStr::new(
                            unit.value().as_str().to_ascii_lowercase().as_str(),
                            unit.span(),
                        );
                        attr.unit = Some(unit);
                    } else if meta.path.is_ident("rename") {
                        let rename = meta.value()?.parse::<syn::LitStr>()?;

                        if attr.rename.is_some() {
                            return Err(syn::Error::new(
                                meta.path.span(),
                                "Duplicate `rename` attribute",
                            ));
                        }

                        attr.rename = Some(rename);
                    } else if meta.path.is_ident("skip") {
                        if attr.skip {
                            return Err(syn::Error::new(
                                meta.path.span(),
                                "Duplicate `skip` attribute",
                            ));
                        }
                        attr.skip = true;
                    } else {
                        panic!("Attributes other than `unit` and `rename` should not reach here");
                    }
                    Ok(())
                })?;
                Ok(attr)
            }
            _ => Err(syn::Error::new(
                span,
                r#"Unknown attribute, expected `#[doc(...)]` or `#[registrant(<key>[=value], ...)]`"#,
            )),
        }
    }
}
