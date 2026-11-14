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

/// An insertion-ordered hash set used inside [`SystemAccessMeta`] to track unique component and
/// resource access [`TypeId`]s.
///
/// [`SystemAccessMeta`]: crate::app::system::SystemAccessMeta
/// [`TypeId`]: std::any::TypeId
pub struct AccessHashSet<T: Eq + Hash> {
    pub(crate) set: IndexSet<T, FxBuildHasher>,
}

impl<T: Eq + Hash> AccessHashSet<T> {
    /// Creates an empty [`AccessHashSet`].
    ///
    /// [`AccessHashSet`]: Self
    pub fn new() -> Self {
        Self {
            set: IndexSet::default(),
        }
    }

    /// Inserts `val` into the set, returning `true` if it was not already present.
    pub fn insert(&mut self, val: T) -> bool {
        self.set.insert(val)
    }

    /// Returns `true` if the set contains `val`.
    pub fn contains(&self, val: &T) -> bool {
        self.set.contains(val)
    }

    /// Returns an iterator over the elements in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.set.iter()
    }
}

impl<T: Eq + Hash> Default for AccessHashSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A lightweight list wrapper used by [`QueryData`] and [`QueryFilter`] implementations to record
/// component [`TypeId`] accesses during system initialization.
///
/// [`QueryData`]: crate::ecs::query::QueryData
/// [`QueryFilter`]: crate::ecs::query::filter::QueryFilter
/// [`TypeId`]: std::any::TypeId
pub struct AccessVec<T> {
    pub(crate) vec: Vec<T>,
}

impl<T> Default for AccessVec<T> {
    fn default() -> Self {
        Self { vec: Vec::new() }
    }
}

impl<T: Eq> AccessVec<T> {
    /// Returns `true` if the list contains `val`.
    pub fn contains(&self, val: &T) -> bool {
        self.vec.contains(val)
    }
}

impl<T> AccessVec<T> {
    /// Creates an empty [`AccessVec`].
    ///
    /// [`AccessVec`]: Self
    pub fn new() -> Self {
        Self { vec: Vec::new() }
    }

    /// Appends `val` to the end of the list.
    pub fn push(&mut self, val: T) {
        self.vec.push(val);
    }

    /// Returns an iterator over the elements in the list.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.vec.iter()
    }

    /// Returns a slice containing all elements in the list.
    pub fn as_slice(&self) -> &[T] {
        self.vec.as_slice()
    }
}

/// Metadata describing a [`System`]'s component/resource read and write accesses, query filters,
/// and thread-safety (`Send`) requirements.
///
/// Populated during [`SystemParam::init_access`] and used by [`SystemExecutor`] implementations to
/// detect conflicting data accesses and schedule non-conflicting systems in parallel.
///
/// [`System`]: crate::app::system::System
/// [`SystemParam::init_access`]: crate::app::system::SystemParam::init_access
/// [`SystemExecutor`]: crate::app::schedule::SystemExecutor
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
    /// Creates a new [`SystemAccessMeta`] for `system_name` with empty access sets and `is_send`
    /// initialized to `true`.
    ///
    /// [`SystemAccessMeta`]: Self
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

    /// Returns the name of the system associated with this metadata.
    pub fn system_name(&self) -> &str {
        &self.system_name
    }

    /// Marks the system as requiring execution on the main thread (for example when requesting
    /// [`NonSendRes`] or [`NonSendResMut`]).
    ///
    /// [`NonSendRes`]: crate::ecs::resources::NonSendRes
    /// [`NonSendResMut`]: crate::ecs::resources::NonSendResMut
    pub fn set_non_send(&mut self) {
        self.is_send = false;
    }

    /// Returns `true` if the system can be safely executed across worker threads.
    pub fn is_send(&self) -> bool {
        self.is_send
    }

    /// Records shared read access to the [`Component`] identified by `type_id`.
    ///
    /// [`Component`]: crate::ecs::component::Component
    pub fn add_component_read(&mut self, type_id: TypeId) {
        self.component_reads.insert(type_id);
    }

    /// Records exclusive write access to the [`Component`] identified by `type_id`.
    ///
    /// [`Component`]: crate::ecs::component::Component
    pub fn add_component_write(&mut self, type_id: TypeId) {
        self.component_writes.insert(type_id);
    }

    /// Records shared read access to the [`Resource`] identified by `type_id`.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn add_resource_read(&mut self, type_id: TypeId) {
        self.resource_reads.insert(type_id);
    }

    /// Records exclusive write access to the [`Resource`] identified by `type_id`.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn add_resource_write(&mut self, type_id: TypeId) {
        self.resource_writes.insert(type_id);
    }

    /// Records a [`With`] inclusion filter constraint for the [`Component`] identified by `type_id`.
    ///
    /// [`With`]: crate::ecs::query::filter::With
    /// [`Component`]: crate::ecs::component::Component
    pub fn add_with_filter(&mut self, type_id: TypeId) {
        self.with_filters.insert(type_id);
    }

    /// Records a [`Without`] exclusion filter constraint for the [`Component`] identified by
    /// `type_id`.
    ///
    /// [`Without`]: crate::ecs::query::filter::Without
    /// [`Component`]: crate::ecs::component::Component
    pub fn add_without_filter(&mut self, type_id: TypeId) {
        self.without_filters.insert(type_id);
    }

    /// Returns `true` if shared read access has been recorded for component `type_id`.
    pub fn has_component_read(&self, type_id: &TypeId) -> bool {
        self.component_reads.contains(type_id)
    }

    /// Returns `true` if exclusive write access has been recorded for component `type_id`.
    pub fn has_component_write(&self, type_id: &TypeId) -> bool {
        self.component_writes.contains(type_id)
    }

    /// Returns `true` if shared read access has been recorded for resource `type_id`.
    pub fn has_resource_read(&self, type_id: &TypeId) -> bool {
        self.resource_reads.contains(type_id)
    }

    /// Returns `true` if exclusive write access has been recorded for resource `type_id`.
    pub fn has_resource_write(&self, type_id: &TypeId) -> bool {
        self.resource_writes.contains(type_id)
    }

    /// Returns `true` if a [`With`] filter constraint has been recorded for component `type_id`.
    ///
    /// [`With`]: crate::ecs::query::filter::With
    pub fn has_with_filter(&self, type_id: &TypeId) -> bool {
        self.with_filters.contains(type_id)
    }

    /// Returns `true` if a [`Without`] filter constraint has been recorded for component `type_id`.
    ///
    /// [`Without`]: crate::ecs::query::filter::Without
    pub fn has_without_filter(&self, type_id: &TypeId) -> bool {
        self.without_filters.contains(type_id)
    }
}

impl SystemAccessMeta {
    /// Returns an iterator over all [`Component`] [`TypeId`]s read by this system.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`TypeId`]: std::any::TypeId
    pub fn component_reads(&self) -> impl Iterator<Item = &TypeId> {
        self.component_reads.iter()
    }

    /// Returns an iterator over all [`Component`] [`TypeId`]s written by this system.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`TypeId`]: std::any::TypeId
    pub fn component_writes(&self) -> impl Iterator<Item = &TypeId> {
        self.component_writes.iter()
    }

    /// Returns an iterator over all [`Resource`] [`TypeId`]s read by this system.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`TypeId`]: std::any::TypeId
    pub fn resource_reads(&self) -> impl Iterator<Item = &TypeId> {
        self.resource_reads.iter()
    }

    /// Returns an iterator over all [`Resource`] [`TypeId`]s written by this system.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`TypeId`]: std::any::TypeId
    pub fn resource_writes(&self) -> impl Iterator<Item = &TypeId> {
        self.resource_writes.iter()
    }

    /// Returns an iterator over all [`Component`] [`TypeId`]s required via [`With`] filters.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`TypeId`]: std::any::TypeId
    /// [`With`]: crate::ecs::query::filter::With
    pub fn with_filters(&self) -> impl Iterator<Item = &TypeId> {
        self.with_filters.iter()
    }

    /// Returns an iterator over all [`Component`] [`TypeId`]s excluded via [`Without`] filters.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`TypeId`]: std::any::TypeId
    /// [`Without`]: crate::ecs::query::filter::Without
    pub fn without_filters(&self) -> impl Iterator<Item = &TypeId> {
        self.without_filters.iter()
    }
}

impl SystemAccessMeta {
    /// Merges all recorded component accesses, resource accesses, filter constraints, and
    /// non-`Send` requirements from `system_access_meta_other` into `self`.
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

/// A type-erased wrapper around a boxed [`System`] stored inside a [`SystemsSchedule`].
///
/// [`System`]: crate::app::system::System
/// [`SystemsSchedule`]: crate::app::schedule::SystemsSchedule
pub struct SystemNode<In: SystemInput, Out> {
    system: Box<dyn System<In = In, Out = Out>>,
}

unsafe impl<I: SystemInput, O> Send for SystemNode<I, O> {}
unsafe impl<I: SystemInput, O> Sync for SystemNode<I, O> {}

impl<I: SystemInput, O> SystemNode<I, O> {
    /// Wraps `system` in a new [`SystemNode`].
    ///
    /// [`SystemNode`]: Self
    pub fn new(system: impl System<In = I, Out = O> + 'static) -> Self {
        Self {
            system: Box::new(system),
        }
    }

    /// Wraps an existing boxed [`System`] in a new [`SystemNode`].
    ///
    /// [`System`]: crate::app::system::System
    /// [`SystemNode`]: Self
    pub fn new_boxed(system: Box<dyn System<In = I, Out = O>>) -> Self {
        Self { system }
    }

    /// Consumes this node and returns the underlying boxed [`System`].
    ///
    /// [`System`]: crate::app::system::System
    pub fn into_inner(self) -> Box<dyn System<In = I, Out = O>> {
        self.system
    }

    /// Executes the underlying [`System`] once with `input` and `world`.
    ///
    /// [`System`]: crate::app::system::System
    pub fn run(&mut self, input: I::Inner<'_>, world: &World) -> Result<O, RunSystemError> {
        self.system.run(input, world)
    }

    /// Evaluates all [`Condition`] predicates attached to this system, returning `true` if every
    /// condition passes.
    ///
    /// [`Condition`]: crate::app::system::condition::Condition
    pub fn should_run(&mut self, world: &World) -> bool {
        self.system_mut()
            .get_or_init(RunConditionsList::default)
            .conditions()
            .iter()
            .all(|condition| condition(world))
    }

    /// Returns a shared reference to the underlying [`System`].
    ///
    /// [`System`]: crate::app::system::System
    pub fn system(&self) -> &dyn System<In = I, Out = O> {
        &*self.system
    }

    /// Returns a mutable reference to the underlying [`System`].
    ///
    /// [`System`]: crate::app::system::System
    pub fn system_mut(&mut self) -> &mut dyn System<In = I, Out = O> {
        &mut *self.system
    }
}

/// Marker trait for [`SystemParam`] types that can be safely fetched and used concurrently across
/// threads.
///
/// # Safety
/// Anything implementing this trait must guarantee it can be used concurrently from any thread.
///
/// [`SystemParam`]: crate::app::system::SystemParam
pub unsafe trait ParallelSystemParam: SystemParam + Send + Sync {}

/// A parameter that can be extracted from a [`World`] and passed into a [`System`].
///
/// [`World`]: crate::ecs::world::World
/// [`System`]: crate::app::system::System
pub trait SystemParam {
    /// The concrete parameter type yielded when fetched from a [`World`] with lifetime `'world`.
    ///
    /// [`World`]: crate::ecs::world::World
    type Item<'world>: SystemParam;

    /// Records the read and write resource or component accesses required by this parameter.
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

/// An executable unit of application logic that operates on data in a [`World`].
///
/// [`World`]: crate::ecs::world::World
pub trait System: SystemData + Send + Sync {
    /// The external input type passed into [`System::run`].
    ///
    /// [`System::run`]: Self::run
    type In: SystemInput;

    /// The output value returned when the system finishes running.
    type Out;

    /// Executes the system once using `input` and the given [`World`].
    ///
    /// [`World`]: crate::ecs::world::World
    fn run(
        &mut self,
        input: <Self::In as SystemInput>::Inner<'_>,
        world: &World,
    ) -> Result<Self::Out, RunSystemError>;

    /// Returns the unique [`SystemIdentity`] identifying this system instance.
    ///
    /// [`SystemIdentity`]: crate::app::system::SystemIdentity
    fn identity(&self) -> &dyn SystemIdentity;
}

/// Trait implemented by types that can be passed as input to a [`System`].
///
/// [`System`]: crate::app::system::System
pub trait SystemInput {
    /// The borrowed or owned representation of the input passed into [`System::run`].
    ///
    /// [`System::run`]: crate::app::system::System::run
    type Inner<'a>: SystemInput;
}

impl SystemInput for () {
    type Inner<'a> = ();
}

/// A [`System`] implementation wrapping a standard Rust function or closure `F`.
///
/// [`System`]: crate::app::system::System
pub struct FunctionSystem<Marker, In: SystemInput, Out, F: Send + Sync + 'static> {
    pub(crate) function: F,
    pub(crate) data: FunctionData,
    pub(crate) identity: FunctionIdentity,
    pub(crate) _marker: PhantomData<fn(In) -> (Marker, Out)>,
}

unsafe impl<Marker, F: Send + Sync, I: SystemInput, O> Send for FunctionSystem<Marker, I, O, F> {}
unsafe impl<Marker, F: Send + Sync, I: SystemInput, O> Sync for FunctionSystem<Marker, I, O, F> {}

impl<Marker, F: Send + Sync + 'static, I: SystemInput, O> FunctionSystem<Marker, I, O, F> {
    /// Wraps `function` in a new [`FunctionSystem`].
    ///
    /// [`FunctionSystem`]: Self
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

/// Conversion trait for turning a function, closure, or existing [`System`] into a concrete [`System`] type.
///
/// [`System`]: crate::app::system::System
pub trait IntoSystem<Marker> {
    /// The resulting concrete [`System`] type.
    ///
    /// [`System`]: crate::app::system::System
    type SystemType: System;

    /// Converts `self` into a [`System`].
    ///
    /// [`System`]: crate::app::system::System
    fn into_system(self) -> Self::SystemType;
}

/// Type-erased identity metadata used to compare, clone, and display [`System`] instances.
///
/// [`System`]: crate::app::system::System
pub trait SystemIdentity: Send + Sync + 'static {
    /// Returns `self` as a dynamic [`Any`] reference for downcasting.
    ///
    /// [`Any`]: std::any::Any
    fn as_any(&self) -> &dyn Any;

    /// Returns `true` if `self` and `other` refer to the same underlying system.
    fn dyn_system_eq(&self, other: &dyn SystemIdentity) -> bool;

    /// Clones this identity into a new boxed [`SystemIdentity`].
    ///
    /// [`SystemIdentity`]: Self
    fn dyn_clone(&self) -> Box<dyn SystemIdentity>;

    /// Returns a human-readable display name for the system.
    fn display_name(&self) -> &str;
}

/// Concrete [`SystemIdentity`] implementation for [`FunctionSystem`].
///
/// [`SystemIdentity`]: crate::app::system::SystemIdentity
/// [`FunctionSystem`]: crate::app::system::FunctionSystem
#[derive(Clone, Debug)]
pub struct FunctionIdentity {
    /// The [`TypeId`] of the underlying system function or closure.
    ///
    /// [`TypeId`]: std::any::TypeId
    pub type_id: TypeId,

    /// The display name of the [`FunctionSystem`].
    ///
    /// [`FunctionSystem`]: crate::app::system::FunctionSystem
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
