use crate::attributes::system_param::SystemParamFieldArgs;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

pub fn derive_system_param_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data_struct) => &data_struct.fields,
        _ => {
            return syn::Error::new_spanned(name, "SystemParam can only be derived on structs")
                .to_compile_error()
                .into();
        }
    };

    let mut item_ty_generics = std::vec::Vec::new();
    for param in input.generics.params.iter() {
        match param {
            syn::GenericParam::Lifetime(_) => {
                let world_lifetime = syn::Lifetime::new("'world", proc_macro2::Span::call_site());
                item_ty_generics.push(quote! { #world_lifetime });
            }
            syn::GenericParam::Type(ty_param) => {
                let ty_ident = &ty_param.ident;
                item_ty_generics.push(quote! { #ty_ident });
            }
            syn::GenericParam::Const(const_param) => {
                let const_ident = &const_param.ident;
                item_ty_generics.push(quote! { #const_ident });
            }
        }
    }
    let item_generics_output = if item_ty_generics.is_empty() {
        quote! {}
    } else {
        quote! { < #( #item_ty_generics ),* > }
    };

    let mut param_types = std::vec::Vec::new();
    let mut param_idents = std::vec::Vec::new();
    let mut ignored_idents = std::vec::Vec::new();

    let is_named = std::matches!(fields, Fields::Named(_));
    let is_unnamed = std::matches!(fields, Fields::Unnamed(_));

    match fields {
        Fields::Named(fields_named) => {
            for field in &fields_named.named {
                let ident = field.ident.clone().unwrap();
                let field_args =
                    match SystemParamFieldArgs::parse_from_field_attributes(&field.attrs) {
                        std::result::Result::Ok(args) => args,
                        std::result::Result::Err(err) => return err.to_compile_error().into(),
                    };

                if field_args.ignore {
                    ignored_idents.push(ident);
                } else {
                    param_types.push(field.ty.clone());
                    param_idents.push(ident);
                }
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for (i, field) in fields_unnamed.unnamed.iter().enumerate() {
                let dummy_ident =
                    syn::Ident::new(&std::format!("field_{}", i), proc_macro2::Span::call_site());
                let field_args =
                    match SystemParamFieldArgs::parse_from_field_attributes(&field.attrs) {
                        std::result::Result::Ok(args) => args,
                        std::result::Result::Err(err) => return err.to_compile_error().into(),
                    };

                if field_args.ignore {
                    ignored_idents.push(dummy_ident);
                } else {
                    param_types.push(field.ty.clone());
                    param_idents.push(dummy_ident);
                }
            }
        }
        Fields::Unit => {}
    }

    let extract_fields = if is_named {
        quote! {
            #(
                let #param_idents = <#param_types as ::avilex::app::system::SystemParam>::get_param(world);
            )*
            #(
                let #param_idents = #param_idents?;
            )*
            std::result::Result::Ok(#name {
                #( #param_idents, )*
                #( #ignored_idents: std::default::Default::default(), )*
            })
        }
    } else if is_unnamed {
        let mut tuple_bindings = std::vec::Vec::new();
        let mut tuple_checks = std::vec::Vec::new();
        let mut tuple_constructors = std::vec::Vec::new();
        let mut param_idx = 0;

        for (i, field) in fields.iter().enumerate() {
            let field_args = match SystemParamFieldArgs::parse_from_field_attributes(&field.attrs) {
                std::result::Result::Ok(args) => args,
                std::result::Result::Err(err) => return err.to_compile_error().into(),
            };

            if field_args.ignore {
                tuple_constructors.push(quote! { std::default::Default::default() });
            } else {
                let ty = &param_types[param_idx];
                let binding_ident =
                    syn::Ident::new(&std::format!("param_{}", i), proc_macro2::Span::call_site());
                tuple_bindings.push(quote! {
                    let #binding_ident = <#ty as ::avilex::app::system::SystemParam>::get_param(world);
                });
                tuple_checks.push(quote! {
                    let #binding_ident = #binding_ident?;
                });
                tuple_constructors.push(quote! { #binding_ident });
                param_idx += 1;
            }
        }

        quote! {
            #( #tuple_bindings )*
            #( #tuple_checks )*
            std::result::Result::Ok(#name( #( #tuple_constructors ),* ))
        }
    } else {
        quote! { std::result::Result::Ok(#name) }
    };

    let expanded = quote! {
        impl #impl_generics ::avilex::app::system::SystemParam for #name #ty_generics #where_clause {
            type Item<'world> = #name #item_generics_output;

            fn init_access(system_meta: &mut ::avilex::app::system::SystemAccessMeta) {
                #(
                    <#param_types as ::avilex::app::system::SystemParam>::init_access(system_meta);
                )*
            }

            #[inline(always)]
            fn get_param<'world>(
                world: &'world ::avilex::ecs::world::World,
            ) -> std::result::Result<Self::Item<'world>, ::avilex::app::system::system_error::SystemParamError> {
                #extract_fields
            }
        }
    };

    TokenStream::from(expanded)
}
