use crate::attributes::schedule_label::ScheduleLabelAttribute;
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_schedule_label_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let args = match ScheduleLabelAttribute::parse_from_attributes(&input.attrs) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let default_executor_fn = args.default_executor.map(|executor_expr| {
        quote! {
            fn default_executor(&self) -> Box<dyn avilex::app::schedule::SystemExecutor> {
                avilex::app::schedule::IntoDefaultExecutor::into_executor(#executor_expr, self)
            }
        }
    });

    let expanded = quote! {
        impl #impl_generics avilex::app::schedule::ScheduleLabel for #name #ty_generics #where_clause {
            #default_executor_fn

            fn dyn_clone(&self) -> Box<dyn avilex::app::schedule::ScheduleLabel> {
                Box::new(self.clone())
            }
        }
    };

    TokenStream::from(expanded)
}
