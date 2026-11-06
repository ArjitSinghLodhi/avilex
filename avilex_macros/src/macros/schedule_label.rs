use crate::attributes::schedule_label::ScheduleLabelAttribute;
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_schedule_label_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let args = ScheduleLabelAttribute::parse(&input.attrs);

    let executor_instantiation = match args.default_executor {
        Some(executor_expr) => quote! {
            Box::new((#executor_expr)(self))
        },
        None => quote! {
            Box::new(avilex::app::schedule::MultiThreadedExecutor::new())
        },
    };

    let expanded = quote! {
        impl #impl_generics avilex::app::schedule::ScheduleLabel for #name #ty_generics #where_clause {
            fn default_executor(&mut self) -> Box<dyn avilex::app::schedule::SystemExecutor> {
                #executor_instantiation
            }

            fn dyn_clone(&self) -> Box<dyn avilex::app::schedule::ScheduleLabel> {
                Box::new(self.clone())
            }
        }
    };

    TokenStream::from(expanded)
}
