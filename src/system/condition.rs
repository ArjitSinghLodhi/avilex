use crate::{
    resources::{ParallelResourceAccessor, Resource},
    system::{ParallelSystemParam, SystemAccessMeta, SystemParam},
    world::World,
};

/// A boxed, thread-safe run condition closure evaluated against a [`World`].
///
/// [`World`]: crate::ecs::world::World
pub type ConditionFn = Box<dyn Fn(&World) -> bool + Send + Sync>;

/// An ordered collection of [`ConditionFn`] predicates evaluated before a system or system set runs.
///
/// [`ConditionFn`]: crate::app::system::condition::ConditionFn
#[derive(Default)]
pub struct RunConditionsList {
    runtime_gates: Vec<ConditionFn>,
}

impl RunConditionsList {
    /// Returns a slice of all registered [`ConditionFn`] predicates.
    ///
    /// [`ConditionFn`]: crate::app::system::condition::ConditionFn
    pub fn conditions(&self) -> &Vec<ConditionFn> {
        &self.runtime_gates
    }

    /// Inserts a [`ConditionFn`] predicate at `index`.
    ///
    /// # Panics
    /// Panics if `index > self.conditions().len()`.
    ///
    /// [`ConditionFn`]: crate::app::system::condition::ConditionFn
    pub fn insert(&mut self, index: usize, condition: ConditionFn) {
        self.runtime_gates.insert(index, condition);
    }

    /// Initializes parameter access metadata for `cond` and appends it to the list.
    pub fn push_condition<C, T>(&mut self, cond: C)
    where
        C: Condition<T> + Send + Sync + 'static,
    {
        C::init_condition_access();
        let run_gate: ConditionFn = Box::new(move |world: &World| cond.run(world));
        self.runtime_gates.push(run_gate);
    }
}

/// A trait for runtime evaluation gates that filter system execution.
///
/// Unlike standard systems, conditions are completely decoupled from traditional scheduling
/// constraints. They are treated as pure, out-of-band evaluation flags that can be executed
/// concurrently across multiple threads at any arbitrary time. Because the scheduler provides
/// no order-of-execution or isolation guarantees for conditions, they should rely strictly
/// on thread-safe, non-blocking parallel parameter lookups.
pub trait Condition<T>: Send + Sync + 'static {
    /// Registers any parameter access metadata required before evaluating this condition.
    fn init_condition_access();

    /// Evaluates the condition against `world`, returning `true` if execution should proceed.
    fn run(&self, world: &World) -> bool;
}

impl<F, P> Condition<P> for F
where
    F: Send + Sync + 'static,
    for<'world> P: SystemParam<Item<'world> = P> + ParallelSystemParam,
    for<'world> P::Item<'world>: ParallelSystemParam,
    for<'world> F: Fn(P) -> bool,
{
    fn init_condition_access() {
        let mut meta = SystemAccessMeta::new("ConditionAccess".to_string());
        P::init_access(&mut meta);
    }

    fn run(&self, world: &World) -> bool {
        match P::get_param(world) {
            Ok(param_data) => (self)(param_data),
            Err(_) => false,
        }
    }
}

/// Wraps a [`Condition`] and returns a new condition that evaluates to its logical negation.
///
/// [`Condition`]: crate::app::system::condition::Condition
pub fn not<P, C>(condition: C) -> impl Fn(&World) -> bool + Clone + Send + Sync
where
    P: 'static,
    C: Condition<P> + 'static + Clone,
{
    move |world: &World| !condition.run(world)
}

/// A run [`Condition`] that returns `true` when a thread-safe [`Resource`] of type `R` exists in
/// the [`World`].
///
/// [`Condition`]: crate::app::system::condition::Condition
/// [`Resource`]: crate::ecs::resources::Resource
/// [`World`]: crate::ecs::world::World
pub fn resource_exists<R>() -> impl Fn(&World) -> bool + Clone + Send + Sync
where
    R: Resource + Send + Sync,
{
    move |world: &World| match <ParallelResourceAccessor<R> as SystemParam>::get_param(world) {
        Ok(resource_accessor) => resource_accessor.is_present(),
        Err(_) => false,
    }
}
