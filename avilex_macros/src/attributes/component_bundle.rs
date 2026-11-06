use syn::{Attribute, Result};

#[derive(Default)]
pub(crate) struct ComponentBundleFieldArgs {
    pub(crate) ignore: bool,
}

impl ComponentBundleFieldArgs {
    pub fn parse_from_field_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut args = Self::default();

        for attr in attrs {
            if attr.path().is_ident("component_bundle") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("ignore") {
                        args.ignore = true;
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized parameter inside component_bundle(...)"))
                    }
                })?;
            }
        }

        Ok(args)
    }
}
