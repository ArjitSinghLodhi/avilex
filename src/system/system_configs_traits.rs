use crate::system::IntoSystem;
use crate::system::System;
use crate::system::SystemIdentity;
use crate::system::condition::{Condition, RunConditionsList};
use crate::system::system_set::SystemSet;
use crate::system::system_storage::SystemExt;
use crate::system::system_traits::AsSchedulingTarget;
use crate::system::system_traits::SystemOrderings;
use crate::system::system_traits::TrackHierarchyEdges;
use std::marker::PhantomData;

#[doc(hidden)]
pub trait ConfigureSystemConfigs<MarkerGroup> {
    fn run_if<P>(self, condition: impl Condition<P> + Clone) -> SystemConfigs<MarkerGroup>;
    fn in_set(self, set: impl SystemSet + Clone) -> SystemConfigs<MarkerGroup>;
    fn before<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<MarkerGroup>;
    fn after<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<MarkerGroup>;
    fn chain(self) -> SystemConfigs<MarkerGroup>;
}

#[doc(hidden)]
pub trait IntoSystemConfigs<MarkerGroup> {
    fn into_configs(self) -> SystemConfigs<MarkerGroup>;
}

impl IntoSystemConfigs<()> for SystemConfigs<()> {
    fn into_configs(self) -> SystemConfigs<()> {
        self
    }
}

#[doc(hidden)]
pub struct SystemConfigs<Marker> {
    pub(crate) systems: Vec<Box<dyn System<In = (), Out = ()>>>,
    pub(crate) _marker: PhantomData<Marker>,
}

impl<Marker> TrackHierarchyEdges<Marker> for SystemConfigs<Marker> {
    fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>) {
        for sys in &self.systems {
            ids.push(sys.identity().dyn_clone());
        }
    }
}

impl<Marker> SystemConfigs<Marker> {
    pub fn new(systems: Vec<Box<dyn System<In = (), Out = ()>>>) -> Self {
        Self {
            systems,
            _marker: PhantomData,
        }
    }
}

macro_rules! impl_system_configs_for_tuples {
    ($($sys:ident),* ; $($marker:ident),*) => {
        impl<$($sys,)* $($marker: 'static,)*> IntoSystemConfigs<($($marker,)*)> for ($($sys,)*)
        where
            $( $sys: IntoSystemConfigs<$marker> + 'static ),*
        {
            fn into_configs(self) -> SystemConfigs<($($marker,)*)> {
                #[allow(non_snake_case)]
                let ($($sys,)*) = self;
                let mut out = Vec::new();
                $( out.extend($sys.into_configs().systems); )*
                SystemConfigs {
                    systems: out,
                    _marker: PhantomData,
                }
            }
        }

        impl<$($sys,)* $($marker,)*> TrackHierarchyEdges<($($marker,)*)> for ($($sys,)*)
        where
            $( $sys: TrackHierarchyEdges<$marker> ),*
        {
            fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>) {
                #[allow(non_snake_case)]
                let ($($sys,)*) = self;
                $( $sys.collect_edge_ids(ids); )*
            }
        }

        impl<$($sys,)* $($marker: 'static,)*> ConfigureSystemConfigs<($($marker,)*)> for ($($sys,)*)
        where
            $( $sys: IntoSystemConfigs<$marker> + TrackHierarchyEdges<$marker> + 'static ),*
        {
            fn run_if<P>(self, condition: impl Condition<P> + Clone) -> SystemConfigs<($($marker,)*)> {
                self.into_configs().run_if(condition)
            }

            fn in_set(self, set: impl SystemSet + Clone) -> SystemConfigs<($($marker,)*)> {
                self.into_configs().in_set(set)
            }

            fn before<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<($($marker,)*)> {
                self.into_configs().before(target)
            }

            fn after<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<($($marker,)*)> {
                self.into_configs().after(target)
            }

            #[allow(unused_assignments)]
            fn chain(self) -> SystemConfigs<($($marker,)*)> {
                #[allow(non_snake_case)]
                let ($($sys,)*) = self;
                let mut flattened_configs = Vec::new();

                let mut previous_edges: Vec<Box<dyn SystemIdentity>> = Vec::new();

                $({
                    let mut current_edges = Vec::new();
                    $sys.collect_edge_ids(&mut current_edges);

                    let mut current_config = $sys.into_configs();

                    if !previous_edges.is_empty() {
                        for sys_item in current_config.systems.iter_mut() {
                            let orderings = sys_item.get_or_init_mut::<SystemOrderings>(SystemOrderings::default);
                            for prev_id in previous_edges.iter() {
                                if !orderings.run_after_systems.iter().any(|id| id.dyn_system_eq(prev_id.as_ref())) {
                                    orderings.run_after_systems.push(prev_id.dyn_clone());
                                }
                            }
                        }
                    }

                    flattened_configs.extend(current_config.systems);
                    previous_edges = current_edges;
                })*

                SystemConfigs {
                    systems: flattened_configs,
                    _marker: PhantomData,
                }
            }
        }

        impl<$($marker,)*> IntoSystemConfigs<($($marker,)*)> for SystemConfigs<($($marker,)*)>
        {
            fn into_configs(self) -> SystemConfigs<($($marker,)*)> {
                self
            }
        }

        impl<$($sys,)* $($marker,)*> AsSchedulingTarget<($($marker,)*)> for ($($sys,)*)
        where
            $( $sys: AsSchedulingTarget<$marker> + 'static ),*
        {
            fn populate_before(&self, orderings: &mut SystemOrderings) {
                #[allow(non_snake_case)]
                let ($($sys,)*) = self;
                $( $sys.populate_before(orderings); )*
            }

            fn populate_after(&self, orderings: &mut SystemOrderings) {
                #[allow(non_snake_case)]

                let ($($sys,)*) = self;
                $( $sys.populate_after(orderings); )*
            }
        }
    };
}

impl_system_configs_for_tuples!(S1 ; M1);
impl_system_configs_for_tuples!(S1, S2 ; M1, M2);
impl_system_configs_for_tuples!(S1, S2, S3 ; M1, M2, M3);
impl_system_configs_for_tuples!(S1, S2, S3, S4; M1, M2, M3, M4);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5; M1, M2, M3, M4, M5);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6; M1, M2, M3, M4, M5, M6);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7; M1, M2, M3, M4, M5, M6, M7);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7, S8; M1, M2, M3, M4, M5, M6, M7, M8);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7, S8, S9; M1, M2, M3, M4, M5, M6, M7, M8, M9);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10; M1, M2, M3, M4, M5, M6, M7, M8, M9, M10);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11; M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11);
impl_system_configs_for_tuples!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12; M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12);

impl<S, Marker: 'static> IntoSystemConfigs<Marker> for S
where
    S: IntoSystem<Marker> + 'static,
    <S as IntoSystem<Marker>>::SystemType: System<In = (), Out = ()>,
{
    fn into_configs(self) -> SystemConfigs<Marker> {
        SystemConfigs {
            systems: vec![Box::new(self.into_system())],
            _marker: PhantomData,
        }
    }
}

impl<Marker> ConfigureSystemConfigs<Marker> for SystemConfigs<Marker> {
    fn run_if<P>(mut self, condition: impl Condition<P> + Clone) -> SystemConfigs<Marker> {
        self.systems.iter_mut().for_each(|sys| {
            sys.get_or_init_mut::<RunConditionsList>(RunConditionsList::default)
                .add_condition(condition.clone());
        });
        self
    }

    fn in_set(mut self, set: impl SystemSet + Clone) -> SystemConfigs<Marker> {
        self.systems.iter_mut().for_each(|sys| {
            sys.get_or_init_mut::<SystemOrderings>(SystemOrderings::default)
                .member_of
                .push(Box::new(set.clone()));
        });
        self
    }

    fn before<T>(mut self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<Marker> {
        self.systems.iter_mut().for_each(|sys| {
            target
                .populate_before(sys.get_or_init_mut::<SystemOrderings>(SystemOrderings::default));
        });
        self
    }

    fn after<T>(mut self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<Marker> {
        self.systems.iter_mut().for_each(|sys| {
            target.populate_after(sys.get_or_init_mut::<SystemOrderings>(SystemOrderings::default));
        });
        self
    }

    fn chain(mut self) -> SystemConfigs<Marker> {
        let n = self.systems.len();
        for idx in 1..n {
            let prev_identity = self.systems[idx - 1].identity().dyn_clone();
            let sys_mut = &mut self.systems[idx];
            sys_mut
                .get_or_init_mut::<SystemOrderings>(SystemOrderings::default)
                .run_after_systems
                .push(prev_identity);
        }
        self
    }
}
