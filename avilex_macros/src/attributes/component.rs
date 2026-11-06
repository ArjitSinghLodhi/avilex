use syn::{Attribute, Expr, Meta, Result, token};

#[derive(Default)]
pub struct ComponentArgs {
    pub on_add: Option<Expr>,
    pub on_insert: Option<Expr>,
    pub on_discard: Option<Expr>,
    pub on_remove: Option<Expr>,
}

impl ComponentArgs {
    pub fn parse_from_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut settings = Self::default();

        for attr in attrs {
            if attr.path().is_ident("component")
                && let Meta::List(meta_list) = &attr.meta
            {
                meta_list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("on_add") {
                        if meta.input.peek(token::Eq) {
                            let expr: Expr = meta.value()?.parse()?;
                            settings.on_add = Some(expr);
                        } else {
                            settings.on_add = Some(syn::parse_str("Self::on_add").unwrap());
                        }
                        Ok(())
                    } else if meta.path.is_ident("on_insert") {
                        if meta.input.peek(token::Eq) {
                            let expr: Expr = meta.value()?.parse()?;
                            settings.on_insert = Some(expr);
                        } else {
                            settings.on_insert = Some(syn::parse_str("Self::on_insert").unwrap());
                        }
                        Ok(())
                    } else if meta.path.is_ident("on_discard") {
                        if meta.input.peek(token::Eq) {
                            let expr: Expr = meta.value()?.parse()?;
                            settings.on_discard = Some(expr);
                        } else {
                            settings.on_discard = Some(syn::parse_str("Self::on_discard").unwrap());
                        }
                        Ok(())
                    } else if meta.path.is_ident("on_remove") {
                        if meta.input.peek(token::Eq) {
                            let expr: Expr = meta.value()?.parse()?;
                            settings.on_remove = Some(expr);
                        } else {
                            settings.on_remove = Some(syn::parse_str("Self::on_remove").unwrap());
                        }
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized component attribute setting"))
                    }
                })?;
            }
        }

        Ok(settings)
    }
}
