use syn::{Attribute, Result, punctuated::Punctuated, token::Comma};

#[derive(Default)]
pub struct QueryArgs {
    pub derives: Vec<syn::Path>,
}

impl QueryArgs {
    pub fn parse_from_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut args = Self::default();

        for attr in attrs {
            if attr.path().is_ident("query_data") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("derive") {
                        let content;
                        syn::parenthesized!(content in meta.input);

                        let paths: Punctuated<syn::Path, Comma> =
                            Punctuated::parse_terminated(&content)?;
                        for path in paths {
                            args.derives.push(path);
                        }
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized parameter inside query_data(...)"))
                    }
                })?;
            }
        }

        Ok(args)
    }
}
