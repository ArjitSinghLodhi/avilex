extern crate proc_macro;
mod attributes;
mod macros {
    pub(crate) mod component;
    pub(crate) mod component_bundle;
    pub(crate) mod event;
    pub(crate) mod parallel_system_param;
    pub(crate) mod query_data;
    pub(crate) mod query_filter;
    pub(crate) mod resource;
    pub(crate) mod schedule_label;
    pub(crate) mod states;
    pub(crate) mod system_param;
    pub(crate) mod system_set;
}

use proc_macro::TokenStream;

#[proc_macro_derive(Component, attributes(component))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    macros::component::derive_component_impl(input)
}

#[proc_macro_derive(Resource)]
pub fn derive_resource(input: TokenStream) -> TokenStream {
    macros::resource::derive_resource_impl(input)
}

#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    macros::event::derive_event_impl(input)
}

#[proc_macro_derive(ComponentBundle, attributes(component_bundle))]
pub fn derive_bundle(input: TokenStream) -> TokenStream {
    macros::component_bundle::derive_bundle_impl(input)
}

#[proc_macro_derive(QueryFilter)]
pub fn derive_filter(input: TokenStream) -> TokenStream {
    macros::query_filter::derive_filter_impl(input)
}

#[proc_macro_derive(QueryData, attributes(query_data))]
pub fn derive_world_query(input: TokenStream) -> TokenStream {
    macros::query_data::derive_query_data_impl(input)
}

#[proc_macro_derive(SystemParam, attributes(system_param))]
pub fn derive_system_param(input: TokenStream) -> TokenStream {
    macros::system_param::derive_system_param_impl(input)
}

#[proc_macro_derive(ParallelSystemParam)]
pub fn derive_parallel_system_param(input: TokenStream) -> TokenStream {
    macros::parallel_system_param::derive_parallel_system_param_impl(input)
}

#[proc_macro_derive(ScheduleLabel, attributes(schedule_label))]
pub fn derive_schedule_label(input: TokenStream) -> TokenStream {
    macros::schedule_label::derive_schedule_label_impl(input)
}

#[proc_macro_derive(States)]
pub fn derive_states(input: TokenStream) -> TokenStream {
    macros::states::derive_states_impl(input)
}

#[proc_macro_derive(SystemSet)]
pub fn derive_system_set(input: TokenStream) -> TokenStream {
    macros::system_set::derive_system_set_impl(input)
}
