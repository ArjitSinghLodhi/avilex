use crate::app::system::SystemAccessMeta;
use crate::app::system::SystemParam;
use crate::app::system::system_error::SystemFailError;
use crate::ecs::world::World;
use crate::system::FunctionIdentity;
use crate::system::FunctionSystem;
use crate::system::IntoSystem;
use crate::system::System;
use crate::system::SystemIdentity;
use crate::system::SystemInput;
use crate::system::condition::{Condition, RunConditionsList};
use crate::system::system_configs_traits::ConfigureSystemConfigs;
use crate::system::system_configs_traits::SystemConfigs;
use crate::system::system_error::RunSystemError;
use crate::system::system_set::SystemSet;
use crate::system::system_storage::SystemExt;
use std::any::TypeId;

#[derive(Default)]
#[doc(hidden)]
pub struct SystemOrderings {
    pub(crate) run_after_sets: Vec<Box<dyn SystemSet>>,
    pub(crate) run_before_sets: Vec<Box<dyn SystemSet>>,
    pub(crate) member_of: Vec<Box<dyn SystemSet>>,
    pub(crate) run_after_systems: Vec<Box<dyn SystemIdentity>>,
    pub(crate) run_before_systems: Vec<Box<dyn SystemIdentity>>,
}

#[doc(hidden)]
pub trait AsSchedulingTarget<T> {
    #[doc(hidden)]
    fn populate_before(&self, orderings: &mut SystemOrderings);
    #[doc(hidden)]
    fn populate_after(&self, orderings: &mut SystemOrderings);
}

impl<T: SystemSet + Clone> AsSchedulingTarget<()> for T {
    fn populate_before(&self, orderings: &mut SystemOrderings) {
        orderings.run_before_sets.push(Box::new(self.clone()));
    }

    fn populate_after(&self, orderings: &mut SystemOrderings) {
        orderings.run_after_sets.push(Box::new(self.clone()));
    }
}
#[doc(hidden)]
pub struct FunctionTargetMarker<P, F>(std::marker::PhantomData<(P, F)>);

#[doc(hidden)]
pub trait TrackHierarchyEdges<MarkerGroup> {
    #[doc(hidden)]
    fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>);
}

macro_rules! impl_system_for_functions {
    ($($param:ident),*) => {
        impl<$($param,)* F: Send + Sync + 'static> System for FunctionSystem<($($param,)*), (), (), F>
        where
            $( $param: SystemParam, )*
            F: Fn($($param),*) + 'static,
            for<'world> F: Fn($($param::Item<'world>,)*)
        {
            type In = ();
            type Out = ();

            fn run(&mut self, _input: <Self::In as SystemInput>::Inner<'_>, world: &World) -> Result<Self::Out, RunSystemError> {
                $(
                    #[allow(non_snake_case)]
                    let $param = <$param>::get_param(world);
                )*

                $(
                    #[allow(non_snake_case)]
                    let $param = match $param {
                        Ok(val) => val,
                        Err(err) => {
                            if err.is_skipped() {
                                return Err(RunSystemError::Skipped(err));
                            } else {
                                let system_name = self.identity().display_name().to_string();
                                let message = err.message().to_string();
                                let severity = err.severity();
                                return Err(RunSystemError::Failed(SystemFailError::new(
                                    system_name,
                                    Box::new(err),
                                    message,
                                    severity,
                                )));
                            }
                        }
                    };
                )*

                #[allow(non_snake_case)]
                Ok((self.function)($($param),*))
            }

            fn identity(&self) -> &dyn SystemIdentity {
                &self.identity
            }
        }

        impl<$($param,)* F: Send + Sync + 'static> IntoSystem<($($param,)*)> for F
        where
            $( $param: SystemParam, )*
            F: Fn($($param,)*),
            for<'world> F: Fn($($param::Item<'world>),*),
        {
            type SystemType = FunctionSystem<($($param,)*), (), (), F>;
            fn into_system(self) -> Self::SystemType {
                let mut system_access_meta = SystemAccessMeta::new(std::any::type_name::<F>().to_string());
                $(
                    #[allow(non_snake_case)]
                    <$param>::init_access(&mut system_access_meta);
                )*

                let mut system = FunctionSystem::new(self);
                system.insert(system_access_meta);
                system
            }
        }

        impl<$($param,)* F: Send + Sync + 'static> ConfigureSystemConfigs<($($param,)*)> for F
        where
            $( $param: SystemParam, )*
            <F as IntoSystem<($($param,)*)>>::SystemType: ConfigureSystemConfigs<($($param,)*)>,
            F: Fn($($param,)*),
            for<'world> F: Fn($($param::Item<'world>),*),
        {
            fn before<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<($($param,)*)> {
                self.into_system().before(target)
            }

            fn after<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<($($param,)*)> {
                self.into_system().after(target)
            }

            fn run_if<P>(self, condition: impl Condition<P> + Clone) -> SystemConfigs<($($param,)*)> {
                self.into_system().run_if(condition)
            }

            fn in_set(self, set: impl SystemSet + Clone + Clone) -> SystemConfigs<($($param,)*)> {
                self.into_system().in_set(set)
            }

            fn chain(self) -> SystemConfigs<($($param,)*)> {
                self.into_system().chain()
            }
        }

        impl<$($param,)* F> AsSchedulingTarget<FunctionTargetMarker<($($param,)*), F>> for F
        where
            $( $param: SystemParam + 'static, )*
            F: Fn($($param),*) + 'static,
            for<'world> F: Fn($($param::Item<'world>,)*)
        {
            fn populate_before(&self, orderings: &mut SystemOrderings) {
                orderings.run_before_systems.push(Box::new(FunctionIdentity {
                    type_id: TypeId::of::<F>(),
                    name: std::any::type_name::<F>().to_string(),
                }));
            }

            fn populate_after(&self, orderings: &mut SystemOrderings) {
                orderings.run_after_systems.push(Box::new(FunctionIdentity {
                    type_id: TypeId::of::<F>(),
                    name: std::any::type_name::<F>().to_string(),
                }));
            }
        }

        impl<$($param,)* F> TrackHierarchyEdges<($($param,)*)> for F
        where
            $( $param: SystemParam + 'static, )*
            F: Fn($($param),*) + 'static,
            for<'world> F: Fn($($param::Item<'world>,)*)
        {
            fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>) {
                ids.push(Box::new(FunctionIdentity {
                    type_id: TypeId::of::<Self>(),
                    name: std::any::type_name::<Self>().to_string(),
                }));
            }
        }
    };
}

impl_system_for_functions!(A);
impl_system_for_functions!(A, B);
impl_system_for_functions!(A, B, C);
impl_system_for_functions!(A, B, C, D);
impl_system_for_functions!(A, B, C, D, E);
impl_system_for_functions!(A, B, C, D, E, G);
impl_system_for_functions!(A, B, C, D, E, G, H);
impl_system_for_functions!(A, B, C, D, E, G, H, I);
impl_system_for_functions!(A, B, C, D, E, G, H, I, J);
impl_system_for_functions!(A, B, C, D, E, G, H, I, J, K);
impl_system_for_functions!(A, B, C, D, E, G, H, I, J, K, L);
impl_system_for_functions!(A, B, C, D, E, G, H, I, J, K, L, M);

impl<F: Send + Sync> IntoSystem<()> for F
where
    F: Fn() + 'static,
{
    type SystemType = FunctionSystem<(), (), (), F>;
    fn into_system(self) -> Self::SystemType {
        FunctionSystem::new(self)
    }
}

impl<F: Send + Sync> AsSchedulingTarget<FunctionTargetMarker<(), F>> for F
where
    F: Fn() + 'static,
{
    fn populate_before(&self, orderings: &mut SystemOrderings) {
        orderings
            .run_before_systems
            .push(Box::new(FunctionIdentity {
                type_id: TypeId::of::<F>(),
                name: std::any::type_name::<F>().to_string(),
            }));
    }

    fn populate_after(&self, orderings: &mut SystemOrderings) {
        orderings.run_after_systems.push(Box::new(FunctionIdentity {
            type_id: TypeId::of::<F>(),
            name: std::any::type_name::<F>().to_string(),
        }));
    }
}

impl<F: Send + Sync> TrackHierarchyEdges<()> for F
where
    F: Fn() + 'static,
{
    fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>) {
        ids.push(Box::new(FunctionIdentity {
            type_id: TypeId::of::<Self>(),
            name: std::any::type_name::<Self>().to_string(),
        }));
    }
}

impl<F: Send + Sync + 'static> ConfigureSystemConfigs<()> for F
where
    F: Fn(),
{
    fn run_if<P>(self, condition: impl Condition<P> + Clone) -> SystemConfigs<()> {
        self.into_system().run_if(condition)
    }

    fn in_set(self, set: impl SystemSet + Clone) -> SystemConfigs<()> {
        self.into_system().in_set(set)
    }

    fn before<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<()> {
        self.into_system().before(target)
    }

    fn after<T>(self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<()> {
        self.into_system().after(target)
    }

    fn chain(self) -> SystemConfigs<()> {
        self.into_system().chain()
    }
}

impl<F: Send + Sync> System for FunctionSystem<(), (), (), F>
where
    F: Fn() + 'static,
{
    type In = ();
    type Out = ();
    fn run(
        &mut self,
        _input: <Self::In as SystemInput>::Inner<'_>,
        _world: &World,
    ) -> Result<Self::Out, RunSystemError> {
        (self.function)();
        Ok(())
    }

    fn identity(&self) -> &dyn SystemIdentity {
        &self.identity
    }
}

impl<Marker, F: Send + Sync, I: SystemInput, O> IntoSystem<Marker>
    for FunctionSystem<Marker, I, O, F>
where
    Self: System,
{
    type SystemType = FunctionSystem<Marker, I, O, F>;
    fn into_system(self) -> Self::SystemType {
        self
    }
}

impl<Marker: 'static, F: Send + Sync + 'static> ConfigureSystemConfigs<Marker>
    for FunctionSystem<Marker, (), (), F>
where
    Self: System<In = (), Out = ()>,
{
    fn run_if<P>(mut self, condition: impl Condition<P> + Clone) -> SystemConfigs<Marker> {
        self.get_or_init_mut::<RunConditionsList>(RunConditionsList::default)
            .add_condition(condition);
        SystemConfigs::new(vec![Box::new(self)])
    }

    fn in_set(mut self, set: impl SystemSet + Clone) -> SystemConfigs<Marker> {
        self.get_or_init_mut::<SystemOrderings>(SystemOrderings::default)
            .member_of
            .push(Box::new(set));
        SystemConfigs::new(vec![Box::new(self)])
    }

    fn before<T>(mut self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<Marker> {
        target.populate_before(self.get_or_init_mut::<SystemOrderings>(SystemOrderings::default));
        SystemConfigs::new(vec![Box::new(self)])
    }

    fn after<T>(mut self, target: impl AsSchedulingTarget<T> + Clone) -> SystemConfigs<Marker> {
        target.populate_after(self.get_or_init_mut::<SystemOrderings>(SystemOrderings::default));
        SystemConfigs::new(vec![Box::new(self)])
    }

    fn chain(self) -> SystemConfigs<Marker> {
        SystemConfigs::new(vec![Box::new(self)])
    }
}

impl<Marker, F: Send + Sync, I: SystemInput, O> TrackHierarchyEdges<Marker>
    for FunctionSystem<Marker, I, O, F>
where
    Self: System,
{
    fn collect_edge_ids(&self, ids: &mut Vec<Box<dyn SystemIdentity>>) {
        ids.push(Box::new(FunctionIdentity {
            type_id: TypeId::of::<F>(),
            name: std::any::type_name::<F>().to_string(),
        }));
    }
}
