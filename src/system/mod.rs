pub(crate) mod condition;
pub(crate) mod system_configs_traits;
pub(crate) mod system_error;
pub(crate) mod system_set;
pub(crate) mod system_set_traits;
pub(crate) mod system_storage;
pub(crate) mod system_traits;

use std::{
    any::{Any, TypeId, type_name},
    hash::Hash,
    marker::PhantomData,
};

use indexmap::IndexSet;
use rustc_hash::FxBuildHasher;

use crate::{
    app::system::{SystemData, SystemExt},
    system::{
        condition::RunConditionsList,
        system_error::{RunSystemError, SystemParamError},
        system_storage::FunctionData,
    },
    world::World,
};

pub struct AccessHashSet<T: Eq + Hash> {
    pub(crate) set: IndexSet<T, FxBuildHasher>,
}

impl<T: Eq + Hash> AccessHashSet<T> {
    pub fn new() -> Self {
        Self {
            set: IndexSet::default(),
        }
    }

    pub fn insert(&mut self, val: T) -> bool {
        self.set.insert(val)
    }

    pub fn contains(&self, val: &T) -> bool {
        self.set.contains(val)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.set.iter()
    }
}

impl<T: Eq + Hash> Default for AccessHashSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AccessVec<T> {
    pub(crate) vec: Vec<T>,
}

impl<T> Default for AccessVec<T> {
    fn default() -> Self {
        Self { vec: Vec::new() }
    }
}

impl<T: Eq> AccessVec<T> {
    pub fn contains(&self, val: &T) -> bool {
        self.vec.contains(val)
    }
}

impl<T> AccessVec<T> {
    pub fn new() -> Self {
        Self { vec: Vec::new() }
    }

    pub fn push(&mut self, val: T) {
        self.vec.push(val);
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.vec.iter()
    }
    pub fn as_slice(&self) -> &[T] {
        self.vec.as_slice()
    }
}

pub struct SystemAccessMeta {
    system_name: String,
    component_reads: AccessHashSet<TypeId>,
    component_writes: AccessHashSet<TypeId>,
    resource_reads: AccessHashSet<TypeId>,
    resource_writes: AccessHashSet<TypeId>,
    with_filters: AccessHashSet<TypeId>,
    without_filters: AccessHashSet<TypeId>,
    is_send: bool,
}

impl Default for SystemAccessMeta {
    fn default() -> Self {
        Self::new("DefaultSystemAccessMeta".to_string())
    }
}

impl SystemAccessMeta {
    pub fn new(system_name: String) -> Self {
        Self {
            system_name,
            component_reads: AccessHashSet::new(),
            component_writes: AccessHashSet::new(),
            resource_reads: AccessHashSet::new(),
            resource_writes: AccessHashSet::new(),
            with_filters: AccessHashSet::new(),
            without_filters: AccessHashSet::new(),
            is_send: true,
        }
    }

    pub fn system_name(&self) -> &str {
        &self.system_name
    }

    pub fn set_non_send(&mut self) {
        self.is_send = false;
    }

    pub fn is_send(&self) -> bool {
        self.is_send
    }

    pub fn add_component_read(&mut self, type_id: TypeId) {
        self.component_reads.insert(type_id);
    }

    pub fn add_component_write(&mut self, type_id: TypeId) {
        self.component_writes.insert(type_id);
    }

    pub fn add_resource_read(&mut self, type_id: TypeId) {
        self.resource_reads.insert(type_id);
    }

    pub fn add_resource_write(&mut self, type_id: TypeId) {
        self.resource_writes.insert(type_id);
    }

    pub fn add_with_filter(&mut self, type_id: TypeId) {
        self.with_filters.insert(type_id);
    }

    pub fn add_without_filter(&mut self, type_id: TypeId) {
        self.without_filters.insert(type_id);
    }

    pub fn has_component_read(&self, type_id: &TypeId) -> bool {
        self.component_reads.contains(type_id)
    }

    pub fn has_component_write(&self, type_id: &TypeId) -> bool {
        self.component_writes.contains(type_id)
    }

    pub fn has_resource_read(&self, type_id: &TypeId) -> bool {
        self.resource_reads.contains(type_id)
    }

    pub fn has_resource_write(&self, type_id: &TypeId) -> bool {
        self.resource_writes.contains(type_id)
    }

    pub fn has_with_filter(&self, type_id: &TypeId) -> bool {
        self.with_filters.contains(type_id)
    }

    pub fn has_without_filter(&self, type_id: &TypeId) -> bool {
        self.without_filters.contains(type_id)
    }
}

impl SystemAccessMeta {
    pub fn component_reads(&self) -> impl Iterator<Item = &TypeId> {
        self.component_reads.iter()
    }

    pub fn component_writes(&self) -> impl Iterator<Item = &TypeId> {
        self.component_writes.iter()
    }

    pub fn resource_reads(&self) -> impl Iterator<Item = &TypeId> {
        self.resource_reads.iter()
    }

    pub fn resource_writes(&self) -> impl Iterator<Item = &TypeId> {
        self.resource_writes.iter()
    }

    pub fn with_filters(&self) -> impl Iterator<Item = &TypeId> {
        self.with_filters.iter()
    }

    pub fn without_filters(&self) -> impl Iterator<Item = &TypeId> {
        self.without_filters.iter()
    }
}

impl SystemAccessMeta {
    pub fn extend_access(&mut self, system_access_meta_other: &mut SystemAccessMeta) {
        self.component_reads
            .set
            .append(&mut system_access_meta_other.component_reads.set);
        self.component_writes
            .set
            .append(&mut system_access_meta_other.component_writes.set);
        self.with_filters
            .set
            .append(&mut system_access_meta_other.with_filters.set);
        self.without_filters
            .set
            .append(&mut system_access_meta_other.without_filters.set);
        self.resource_reads
            .set
            .append(&mut system_access_meta_other.resource_reads.set);
        self.resource_writes
            .set
            .append(&mut system_access_meta_other.resource_writes.set);
        if !system_access_meta_other.is_send {
            self.is_send = false;
        }
    }
}

pub struct SystemNode<In: SystemInput, Out> {
    system: Box<dyn System<In = In, Out = Out>>,
}

unsafe impl<I: SystemInput, O> Send for SystemNode<I, O> {}
unsafe impl<I: SystemInput, O> Sync for SystemNode<I, O> {}

impl<I: SystemInput, O> SystemNode<I, O> {
    pub fn new(system: impl System<In = I, Out = O> + 'static) -> Self {
        Self {
            system: Box::new(system),
        }
    }
    pub fn new_boxed(system: Box<dyn System<In = I, Out = O>>) -> Self {
        Self { system }
    }

    pub fn run(&mut self, input: I::Inner<'_>, world: &World) -> Result<O, RunSystemError> {
        self.system.run(input, world)
    }

    pub fn should_run(&mut self, world: &World) -> bool {
        self.system_mut()
            .get_or_init(RunConditionsList::default)
            .conditions()
            .iter()
            .all(|condition| condition(world))
    }

    pub fn system(&self) -> &dyn System<In = I, Out = O> {
        &*self.system
    }

    pub fn system_mut(&mut self) -> &mut dyn System<In = I, Out = O> {
        &mut *self.system
    }
}

/// # Safety
///
/// Anything implementing this trait must guarantee it can be used concurrently from any thread.
pub unsafe trait ParallelSystemParam: SystemParam + Send + Sync {}

pub trait SystemParam {
    type Item<'world>: SystemParam;
    fn init_access(system_meta: &mut SystemAccessMeta);
    /// Fetches the parameter data from the world.
    ///
    /// Returns `Ok(Item)` if the data is successfully built.
    /// Returns `Err(SystemParamError)` if a condition or layout constraint is not met.
    ///
    /// - `is_skipped == true`: The system cleanly halts and returns `RunSystemError::Skipped`.
    /// - `is_skipped == false`: The system halts and returns `RunSystemError::Failed` with the error context.
    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError>;
}

pub trait System: SystemData + Send + Sync {
    type In: SystemInput;
    type Out;
    fn run(
        &mut self,
        input: <Self::In as SystemInput>::Inner<'_>,
        world: &World,
    ) -> Result<Self::Out, RunSystemError>;
    fn identity(&self) -> &dyn SystemIdentity;
}

pub trait SystemInput {
    type Inner<'a>: SystemInput;
}

impl SystemInput for () {
    type Inner<'a> = ();
}

pub struct FunctionSystem<Marker, In: SystemInput, Out, F: Send + Sync + 'static> {
    pub(crate) function: F,
    pub(crate) data: FunctionData,
    pub(crate) identity: FunctionIdentity,
    pub(crate) _marker: PhantomData<fn(In) -> (Marker, Out)>,
}

unsafe impl<Marker, F: Send + Sync, I: SystemInput, O> Send for FunctionSystem<Marker, I, O, F> {}
unsafe impl<Marker, F: Send + Sync, I: SystemInput, O> Sync for FunctionSystem<Marker, I, O, F> {}

impl<Marker, F: Send + Sync + 'static, I: SystemInput, O> FunctionSystem<Marker, I, O, F> {
    pub fn new(function: F) -> Self {
        Self {
            function,
            data: FunctionData::new(),
            identity: FunctionIdentity {
                type_id: TypeId::of::<F>(),
                name: type_name::<Self>().to_string(),
            },
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Marker, F: Send + Sync + 'static, I: SystemInput, O> SystemData
    for FunctionSystem<Marker, I, O, F>
{
    fn get_raw(&self, id: TypeId) -> Option<&Box<dyn Any + Send + Sync>> {
        self.data.get_raw_data(&id)
    }

    fn get_raw_mut(&mut self, id: TypeId) -> Option<&mut Box<dyn Any + Send + Sync>> {
        self.data.get_raw_data_mut(&id)
    }

    fn insert_raw(&mut self, id: TypeId, value: Box<dyn Any + Send + Sync>) {
        self.data.insert_raw_data(&id, value);
    }

    fn remove_raw(&mut self, id: TypeId) -> Option<Box<dyn Any + Send + Sync + 'static>> {
        self.data.remove_raw(id)
    }
}

pub trait IntoSystem<Marker> {
    type SystemType: System;
    fn into_system(self) -> Self::SystemType;
}

pub trait SystemIdentity: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn dyn_system_eq(&self, other: &dyn SystemIdentity) -> bool;
    fn dyn_clone(&self) -> Box<dyn SystemIdentity>;
    fn display_name(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct FunctionIdentity {
    pub type_id: TypeId,
    pub name: String,
}

impl SystemIdentity for FunctionIdentity {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn dyn_system_eq(&self, other: &dyn SystemIdentity) -> bool {
        if let Some(other_concrete) = other.as_any().downcast_ref::<Self>() {
            self.type_id == other_concrete.type_id
        } else {
            false
        }
    }
    fn dyn_clone(&self) -> Box<dyn SystemIdentity> {
        Box::new(FunctionIdentity {
            type_id: self.type_id,
            name: self.name.clone(),
        })
    }
    fn display_name(&self) -> &str {
        &self.name
    }
}

macro_rules! impl_system_param_tuple {
    ($($param:ident),*) => {
        impl<$($param: SystemParam),*> SystemParam for ($($param,)*) {
            type Item<'world> = ($($param::Item<'world>,)*);

            #[inline]
            fn init_access(system_meta: &mut SystemAccessMeta) {
                $(
                    <$param>::init_access(system_meta);
                )*
            }

            #[inline]
            fn get_param<'world>(
                world: &'world World,
            ) -> Result<Self::Item<'world>, SystemParamError> {
                $(
                    #[allow(non_snake_case)]
                    let $param = <$param>::get_param(world)?;
                )*
                Ok(($($param,)*))
            }
        }
    };
}

impl_system_param_tuple!(A);
impl_system_param_tuple!(A, B);
impl_system_param_tuple!(A, B, C);
impl_system_param_tuple!(A, B, C, D);
impl_system_param_tuple!(A, B, C, D, E);
impl_system_param_tuple!(A, B, C, D, E, G);
impl_system_param_tuple!(A, B, C, D, E, G, H);
impl_system_param_tuple!(A, B, C, D, E, G, H, I);
impl_system_param_tuple!(A, B, C, D, E, G, H, I, J);
impl_system_param_tuple!(A, B, C, D, E, G, H, I, J, K);
impl_system_param_tuple!(A, B, C, D, E, G, H, I, J, K, L);
impl_system_param_tuple!(A, B, C, D, E, G, H, I, J, K, L, M);
