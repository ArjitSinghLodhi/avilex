use syn::{Attribute, Expr, Meta, Result, token};

#[derive(Default)]
pub(crate) struct ScheduleLabelAttribute {
    pub(crate) default_executor: Option<Expr>,
}

impl ScheduleLabelAttribute {
    pub(crate) fn parse_from_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut settings = Self::default();

        for attr in attrs {
            if attr.path().is_ident("schedule_label")
                && let Meta::List(meta_list) = &attr.meta
            {
                meta_list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default_executor") {
                        if meta.input.peek(token::Eq) {
                            let expr = meta.value()?.parse()?;
                            settings.default_executor = Some(expr);
                        } else {
                            settings.default_executor =
                                Some(syn::parse_str("Self::default_executor").unwrap());
                        }
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized schedule_label attribute setting"))
                    }
                })?;
            }
        }

        Ok(settings)
    }
}
