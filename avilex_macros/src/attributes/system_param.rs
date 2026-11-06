use syn::{Attribute, Result};

#[derive(Default, Clone)]
pub struct SystemParamFieldArgs {
    pub ignore: bool,
}

impl SystemParamFieldArgs {
    pub fn parse_from_field_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut args = Self::default();

        for attr in attrs {
            if attr.path().is_ident("system_param") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("ignore") {
                        args.ignore = true;
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized parameter inside system_param(...)"))
                    }
                })?;
            }
        }

        Ok(args)
    }
}
