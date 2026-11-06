use crate::attributes::component::ComponentArgs;
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_component_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let args = ComponentArgs::parse_from_attributes(&input.attrs).unwrap_or_else(|err| {
        panic!(
            "avilexMacroError: Failed to parse component attributes: {}",
            err
        );
    });

    let on_add_body = match args.on_add {
        Some(expr) => quote! { Some(#expr) },
        None => quote! { None },
    };

    let on_insert_body = match args.on_insert {
        Some(expr) => quote! { Some(#expr) },
        None => quote! { None },
    };

    let on_discard_body = match args.on_discard {
        Some(expr) => quote! { Some(#expr) },
        None => quote! { None },
    };

    let on_remove_body = match args.on_remove {
        Some(expr) => quote! { Some(#expr) },
        None => quote! { None },
    };

    let expanded = quote! {
        impl #impl_generics ::avilex::ecs::component::Component for #struct_name #ty_generics #where_clause {
            #[inline(always)]
            fn on_add() -> Option<::avilex::ecs::component::hooks::ComponentHook> {
                #on_add_body
            }

            #[inline(always)]
            fn on_insert() -> Option<::avilex::ecs::component::hooks::ComponentHook> {
                #on_insert_body
            }

            #[inline(always)]
            fn on_discard() -> Option<::avilex::ecs::component::hooks::ComponentHook> {
                #on_discard_body
            }

            #[inline(always)]
            fn on_remove() -> Option<::avilex::ecs::component::hooks::ComponentHook> {
                #on_remove_body
            }
        }
    };

    TokenStream::from(expanded)
}
