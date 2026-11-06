use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_system_set_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics avilex::app::system::system_set::SystemSet for #name #ty_generics #where_clause {
            fn dyn_clone(&self) -> std::boxed::Box<dyn avilex::app::system::system_set::SystemSet> {
                Box::new(std::clone::Clone::clone(self))
            }

            fn create_guard_node(
                &self,
                gates: Vec<avilex::app::system::condition::ConditionFn>,
                shared_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> avilex::app::system::SystemNode<(), ()> {
                avilex::app::system::SystemNode::<(), ()>::new(avilex::app::system::system_set::GuardNode::<Self>::new(self.dyn_clone(), gates, shared_flag))
            }
        }
    };

    TokenStream::from(expanded)
}
