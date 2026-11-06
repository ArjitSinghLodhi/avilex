use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_parallel_system_param_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        unsafe impl #impl_generics avilex::app::system::ParallelSystemParam for #name #ty_generics #where_clause {}
    };

    TokenStream::from(expanded)
}
