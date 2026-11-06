use crate::attributes::component_bundle::ComponentBundleFieldArgs;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

pub fn derive_bundle_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data_struct) => &data_struct.fields,
        _ => {
            return syn::Error::new_spanned(name, "ComponentBundle can only be derived on structs")
                .to_compile_error()
                .into();
        }
    };

    let mut types = Vec::new();
    let mut destructure_patterns = Vec::new();
    let mut ignored_patterns = Vec::new();

    match fields {
        Fields::Named(fields_named) => {
            for field in &fields_named.named {
                let f_ident = &field.ident;
                let args = ComponentBundleFieldArgs::parse_from_field_attributes(&field.attrs).unwrap_or_else(|err| {
                    panic!("avilexMacroError: Failed to parse field attributes on component bundle: {}", err);
                });

                if !args.ignore {
                    types.push(field.ty.clone());
                    destructure_patterns.push(quote! { #f_ident });
                } else {
                    ignored_patterns.push(quote! { #f_ident: _ });
                }
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for (i, field) in fields_unnamed.unnamed.iter().enumerate() {
                let dummy_ident =
                    syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site());
                let args = ComponentBundleFieldArgs::parse_from_field_attributes(&field.attrs).unwrap_or_else(|err| {
                    panic!("avilexMacroError: Failed to parse field attributes on component bundle: {}", err);
                });

                if !args.ignore {
                    types.push(field.ty.clone());
                    destructure_patterns.push(quote! { #dummy_ident });
                } else {
                    ignored_patterns.push(quote! { _ });
                }
            }
        }
        Fields::Unit => {}
    }

    let field_count = types.len();

    let destructure = match fields {
        Fields::Named(_) => {
            quote! { let Self { #(#destructure_patterns,)* #(#ignored_patterns),* } = self; }
        }
        Fields::Unnamed(fields_unnamed) => {
            let mut pattern_list = Vec::new();
            let mut active_idx = 0;
            let mut ignored_idx = 0;

            for field in fields_unnamed.unnamed.iter() {
                let args = ComponentBundleFieldArgs::parse_from_field_attributes(&field.attrs).unwrap_or_else(|err| {
                    panic!("avilexMacroError: Failed to parse field attributes on component bundle: {}", err);
                });

                if !args.ignore {
                    pattern_list.push(destructure_patterns[active_idx].clone());
                    active_idx += 1;
                } else {
                    pattern_list.push(ignored_patterns[ignored_idx].clone());
                    ignored_idx += 1;
                }
            }
            quote! { let Self(#(#pattern_list),*) = self; }
        }
        Fields::Unit => quote! { let Self = self; },
    };

    let expanded = quote! {
        impl #impl_generics ::avilex::ecs::commands::bundle::ComponentBundle for #name #ty_generics #where_clause {
            const TYPE_IDS: &'static [::std::any::TypeId] = &[
                #( ::std::any::TypeId::of::<#types>() ),*
            ];

            fn get_type_ids() -> &'static [::std::any::TypeId] {
                Self::TYPE_IDS
            }

            fn register_bundle_hooks(world: &mut ::avilex::ecs::world::World) {
                #(
                    <#types as ::avilex::ecs::commands::bundle::ComponentBundle>::register_bundle_hooks(world);
                )*
            }

            fn create_empty_columns(columns: &mut ::avilex::__private::indexmap::IndexMap<::std::any::TypeId, ::avilex::ecs::world::archetypes::ComponentColumn, ::avilex::__private::rustc_hash::FxBuildHasher>) {
                #(
                    <#types as ::avilex::ecs::commands::bundle::ComponentBundle>::create_empty_columns(columns);
                )*
            }

            unsafe fn insert_to_archetype(self, archetype: &mut ::avilex::ecs::world::archetypes::Archetype, row_idx: usize) {
                #destructure
                unsafe {
                    #(
                        <#types as ::avilex::ecs::commands::bundle::ComponentBundle>::insert_to_archetype(#destructure_patterns, archetype, row_idx);
                    )*
                }
            }

            type NamesArray = [&'static str; #field_count];

            fn get_type_names() -> Self::NamesArray {
                [
                    #( ::std::any::type_name::<#types>() ),*
                ]
            }
        }
    };

    TokenStream::from(expanded)
}
