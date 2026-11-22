use std::{marker::PhantomData, sync::Arc};

use dashmap::DashMap;
use parking_lot::Mutex;
use rustc_hash::FxBuildHasher;

use crate::{
    query::{Query, QueryData, QueryFilter},
    system::{ParallelSystemParam, SystemAccessMeta, SystemParam, system_error::SystemParamError},
    world::{
        World,
        archetypes::{Archetype, ArchetypeId},
    },
};

/// A thread-safe, clonable handle providing detached remote access to the engine's archetype data blocks.
///
/// `ParallelQueryAccessor` can be passed to background worker threads, network loops, or async task pools,
/// allowing them to concurrently read or modify component data arrays completely out-of-band.
///
/// # Concurrency & Performance
/// **All data retrieval and structural handle operations are 100% thread-safe and automatically synchronized.**
/// Background tasks can invoke random slot lookups, clone [`Entity`] handles, or update component states
/// simultaneously across multiple threads without requiring manual synchronization primitives—the engine
/// acquires fine-grained per-column [`RwLock`] guards for the duration of each [`scope`](Self::scope).
///
/// # Initialization Requirement
/// This handle is sourced via [`World::par_query_accessor`] (or as a [`SystemParam`]). **If** the accessor
/// interacts with tracking data (e.g., [`Changed<T>`], [`Added<T>`]), those component trackers must be
/// pre-registered before [`App::build`]. This can be done either by calling [`World::par_query_accessor`]
/// or using it as a [`SystemParam`] pre-build to let the engine auto-register them, or by manually
/// invoking the tracking registerers on [`App`] or [`World`] pre-build—which allows you to safely obtain
/// the accessor post-build.
///
/// # Single-Thread Aliasing Rules
/// While cross-thread parallel processing is completely race-free, interacting with a query from the *same thread*
/// behaves exactly like **Standard Rust borrow rules** enforced at runtime based on your [`QueryData`] parameter layout:
/// * **Exclusive Access (Unique Borrow):** You must not open a query scope that requests mutable access to a component column
///   while another active query scope holds an active borrow (mutable or shared) on that same component column on the same thread.
/// * **Borrow Rules:** Violating standard shared vs. exclusive reference bounds across nested scopes on a single execution
///   path will cause a thread freeze due to nested acquisition of the underlying column-level locks.
///
/// [`Entity`]: crate::ecs::entity::Entity
/// [`RwLock`]: parking_lot::RwLock
/// [`World::par_query_accessor`]: crate::ecs::world::World::par_query_accessor
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
/// [`Added<T>`]: crate::ecs::reactivity::added::Added
/// [`App::build`]: crate::app::App::build
/// [`App`]: crate::app::App
/// [`World`]: crate::ecs::world::World
/// [`QueryData`]: crate::ecs::query::QueryData
pub struct ParallelQueryAccessor<Q: QueryData, F: QueryFilter = ()> {
    pub(crate) archetypes_map: Arc<DashMap<ArchetypeId, Mutex<Archetype>, FxBuildHasher>>,
    pub(crate) _marker: PhantomData<(Q, F)>,
}

impl<Q: QueryData, F: QueryFilter> Clone for ParallelQueryAccessor<Q, F> {
    fn clone(&self) -> Self {
        Self {
            archetypes_map: self.archetypes_map.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Q: QueryData, F: QueryFilter> ParallelQueryAccessor<Q, F> {
    /// Opens a temporary [`Query`] scope over the underlying archetype data, executes `f`, and
    /// releases all column locks when the closure returns.
    ///
    /// [`Query`]: crate::ecs::query::Query
    pub fn scope<Func, R>(&self, f: Func) -> R
    where
        Func: for<'q> FnOnce(Query<'q, Q, F>) -> R,
    {
        let query = Query::new(&self.archetypes_map);
        f(query)
    }
}

unsafe impl<Q: QueryData, F: QueryFilter> Send for ParallelQueryAccessor<Q, F> {}
unsafe impl<Q: QueryData, F: QueryFilter> Sync for ParallelQueryAccessor<Q, F> {}
unsafe impl<Q: QueryData, F: QueryFilter> ParallelSystemParam for ParallelQueryAccessor<Q, F> {}

impl<Q: QueryData, F: QueryFilter> SystemParam for ParallelQueryAccessor<Q, F> {
    type Item<'world> = ParallelQueryAccessor<Q, F>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Query::<Q, F>::init_access(&mut SystemAccessMeta::new(
            system_meta.system_name().to_string(),
        ));
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(ParallelQueryAccessor {
            archetypes_map: world.archetypes_manager.archetypes_map(),
            _marker: PhantomData,
        })
    }
}
