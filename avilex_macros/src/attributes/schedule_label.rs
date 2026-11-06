use syn::{Attribute, Expr};

pub(crate) struct ScheduleLabelAttribute {
    pub(crate) default_executor: Option<Expr>,
}

impl ScheduleLabelAttribute {
    pub(crate) fn parse(attrs: &[Attribute]) -> Self {
        let mut default_executor = None;

        for attr in attrs {
            if attr.path().is_ident("schedule_label") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default_executor") {
                        let expr: Expr = meta.value()?.parse()?;
                        default_executor = Some(expr);
                    }
                    Ok(())
                });
            }
        }

        Self { default_executor }
    }
}
