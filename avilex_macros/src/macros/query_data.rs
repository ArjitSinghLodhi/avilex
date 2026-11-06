use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Index, parse_macro_input};

use crate::attributes::query_data::QueryArgs;

pub fn derive_query_data_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match QueryArgs::parse_from_attributes(&input.attrs) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    let name = &input.ident;
    let visibility = &input.vis;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data_struct) => &data_struct.fields,
        _ => {
            return syn::Error::new_spanned(name, "QueryData can only be derived on structs")
                .to_compile_error()
                .into();
        }
    };

    let mut field_types = Vec::new();
    let mut field_idents = Vec::new();
    let mut field_visibilities = Vec::new();
    let mut tuple_indices = Vec::new();
    let mut scratch_idents = Vec::new();

    let is_named = matches!(fields, Fields::Named(_));
    let is_unnamed = matches!(fields, Fields::Unnamed(_));

    match fields {
        Fields::Named(fields_named) => {
            for (i, field) in fields_named.named.iter().enumerate() {
                let ident = field.ident.clone().unwrap();
                field_types.push(field.ty.clone());
                field_idents.push(ident.clone());
                field_visibilities.push(field.vis.clone());
                tuple_indices.push(Index::from(i));

                scratch_idents.push(syn::Ident::new(
                    &format!("_{}", ident),
                    proc_macro2::Span::call_site(),
                ));
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for (i, field) in fields_unnamed.unnamed.iter().enumerate() {
                field_types.push(field.ty.clone());
                field_visibilities.push(field.vis.clone());
                tuple_indices.push(Index::from(i));

                scratch_idents.push(syn::Ident::new(
                    &format!("_field_{}", i),
                    proc_macro2::Span::call_site(),
                ));
            }
        }
        Fields::Unit => {}
    }

    let link_fields_check = if is_named {
        let mut field_mappings = Vec::new();
        if let Fields::Named(fields_named) = fields {
            for (real_ident, scratch_ident) in fields_named
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .zip(&scratch_idents)
            {
                field_mappings.push(quote! { #real_ident: #scratch_ident });
            }
        }
        quote! {
            if false {
                let #name { #( #field_mappings ),* } = unsafe { ::std::mem::zeroed() };
            }
        }
    } else if is_unnamed {
        quote! {
            if false {
                let #name ( #( #scratch_idents, )* ) = unsafe { ::std::mem::zeroed() };
            }
        }
    } else {
        quote! {}
    };

    let item_struct_name =
        syn::Ident::new(&format!("{}Item", name), proc_macro2::Span::call_site());
    let readonly_item_name = syn::Ident::new(
        &format!("{}ReadOnlyItem", name),
        proc_macro2::Span::call_site(),
    );

    let item_fields = if is_named {
        quote! {
            #( #field_visibilities #field_idents: <#field_types as ::avilex::ecs::query::QueryData>::Item<'w>, )*
        }
    } else {
        quote! {
            #( #field_visibilities <#field_types as ::avilex::ecs::query::QueryData>::Item<'w>, )*
        }
    };

    let readonly_item_fields = if is_named {
        quote! {
            #( #field_visibilities #field_idents: <#field_types as ::avilex::ecs::query::QueryData>::ReadOnlyItem<'w>, )*
        }
    } else {
        quote! {
            #( #field_visibilities <#field_types as ::avilex::ecs::query::QueryData>::ReadOnlyItem<'w>, )*
        }
    };

    let query_derives = &args.derives;
    let derive_macro_tokens = if !query_derives.is_empty() {
        quote! { #[derive(#(#query_derives),*)] }
    } else {
        quote! {}
    };

    let item_struct_def = if is_named {
        quote! {
            #derive_macro_tokens
            #visibility struct #item_struct_name<'w> { #item_fields }
        }
    } else {
        quote! {
            #derive_macro_tokens
            #visibility struct #item_struct_name<'w> ( #item_fields );
        }
    };

    let readonly_item_def = if is_named {
        quote! {
            #derive_macro_tokens
            #visibility struct #readonly_item_name<'w> { #readonly_item_fields }
        }
    } else {
        quote! {
            #derive_macro_tokens
            #visibility struct #readonly_item_name<'w> ( #readonly_item_fields );
        }
    };

    let item_construction = if is_named {
        quote! { #item_struct_name { #( #field_idents: <#field_types as ::avilex::ecs::query::QueryData>::fetch_mut(&fetch.#tuple_indices, index) ),* } }
    } else {
        quote! { #item_struct_name ( #( <#field_types as ::avilex::ecs::query::QueryData>::fetch_mut(&fetch.#tuple_indices, index) ),* ) }
    };

    let readonly_item_construction = if is_named {
        quote! { #readonly_item_name { #( #field_idents: <#field_types as ::avilex::ecs::query::QueryData>::fetch_read_only(&fetch.#tuple_indices, index) ),* } }
    } else {
        quote! { #readonly_item_name ( #( <#field_types as ::avilex::ecs::query::QueryData>::fetch_read_only(&fetch.#tuple_indices, index) ),* ) }
    };

    let expanded = quote! {
        #[allow(dead_code)]
        #item_struct_def

        #[allow(dead_code)]
        #readonly_item_def

        impl #impl_generics ::avilex::ecs::query::QueryData for #name #ty_generics #where_clause {
            type Item<'w> = #item_struct_name<'w>;
            type ReadOnlyItem<'w> = #readonly_item_name<'w>;
            type Fetch<'w> = ( #( <#field_types as ::avilex::ecs::query::QueryData>::Fetch<'w>, )* );

            #[inline(always)]
            fn matches(archetype: &::avilex::ecs::world::archetypes::Archetype) -> bool {
                #link_fields_check
                true #( && <#field_types as ::avilex::ecs::query::QueryData>::matches(archetype) )*
            }

            #[inline(always)]
            unsafe fn init_fetch<'w>(
                archetype: &'w ::avilex::ecs::world::archetypes::Archetype,
            ) -> Self::Fetch<'w> {
                unsafe {
                    ( #( <#field_types as ::avilex::ecs::query::QueryData>::init_fetch(archetype), )* )
                }
            }

            #[inline(always)]
            fn collect_access(
                reads: &mut ::avilex::app::system::AccessVec<::std::any::TypeId>,
                writes: &mut ::avilex::app::system::AccessVec<::std::any::TypeId>,
            ) {
                #(
                    <#field_types as ::avilex::ecs::query::QueryData>::collect_access(reads, writes);
                )*
            }

            #[inline(always)]
            unsafe fn fetch_mut<'w, 'b>(fetch: &Self::Fetch<'b>, index: usize) -> Self::Item<'w> {
                unsafe {
                    #item_construction
                }
            }

            #[inline(always)]
            unsafe fn fetch_read_only<'w, 'b>(fetch: &Self::Fetch<'b>, index: usize) -> Self::ReadOnlyItem<'w> {
                unsafe {
                    #readonly_item_construction
                }
            }
        }
    };
    TokenStream::from(expanded)
}
