pub mod filter;
pub(crate) mod parallel_query;
mod query_data_params;
pub(crate) mod single;

use dashmap::DashMap;
pub use query_data_params::Has;

pub use filter::*;

use parking_lot::{Mutex, RwLockReadGuard};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use rustc_hash::{FxBuildHasher, FxHashSet};
use std::{
    any::{TypeId, type_name},
    mem::transmute,
    sync::Arc,
};

use crate::{
    entity::{ENTITY_LOCATION_REGISTRY, Entity, EntityLocationData},
    query::single::QuerySingleError,
    system::{AccessVec, SystemAccessMeta, SystemParam, system_error::SystemParamError},
    world::{
        World,
        archetypes::{Archetype, ArchetypeId},
    },
};

/// A trait for types that can be fetched as items by a [`Query`].
///
/// [`Query`]: crate::ecs::query::Query
pub trait QueryData {
    /// The mutable item type yielded when iterating or fetching mutably.
    type Item<'w>: Send;

    /// The read-only item type yielded when iterating or fetching immutably.
    type ReadOnlyItem<'w>: Send;

    /// Per-archetype state (such as column pointers or guards) initialized once before row iteration.
    type Fetch<'w>: Sync;

    /// Returns `true` if `archetype` contains all required component columns for this query data.
    ///
    /// [`Archetype`]: crate::ecs::world::archetypes::Archetype
    fn matches(archetype: &Archetype) -> bool;

    /// Initializes the per-archetype [`QueryData::Fetch`] state for `archetype`.
    ///
    /// # Safety
    /// Implementations must guarantee that the returned `Self::Fetch` value contains valid,
    /// initialized metadata, pointers, or references that remain valid for the duration of
    /// the borrow layout. It must not lead to out-of-bounds memory access or invalid type casting.
    ///
    /// [`QueryData::Fetch`]: Self::Fetch
    /// [`Archetype`]: crate::ecs::world::archetypes::Archetype
    fn init_fetch<'w>(archetype: &'w Archetype) -> Self::Fetch<'w>;

    /// Records all component [`TypeId`]s read or written by this query data into `reads` and `writes`.
    ///
    /// [`TypeId`]: std::any::TypeId
    fn collect_access(reads: &mut AccessVec<TypeId>, writes: &mut AccessVec<TypeId>);

    /// Fetches the mutable query item at row `index` from `fetch`.
    ///
    /// # Safety
    /// * `fetch` must be a valid state originally produced by [`QueryData::init_fetch`] for the current archetype.
    /// * `index` must be strictly within the bounds of the allocated entity array for this archetype.
    /// * The caller must ensure that unique, mutable access to the underlying item at `index`
    ///   is maintained globally (no aliasing reads or writes).
    ///
    /// [`QueryData::init_fetch`]: Self::init_fetch
    unsafe fn fetch_mut<'w, 'b>(fetch: &Self::Fetch<'b>, index: usize) -> Self::Item<'w>;

    /// Fetches the read-only query item at row `index` from `fetch`.
    ///
    /// # Safety
    /// * `fetch` must be a valid state originally produced by [`QueryData::init_fetch`].
    /// * `index` must be strictly within bounds.
    /// * Shared access must be synchronized; no concurrent mutable borrows may exist for this item.
    ///
    /// [`QueryData::init_fetch`]: Self::init_fetch
    unsafe fn fetch_read_only<'w, 'b>(
        fetch: &Self::Fetch<'b>,
        index: usize,
    ) -> Self::ReadOnlyItem<'w>;
}

/// A read-only slice of rows within a single matched [`Archetype`], yielded by
/// [`Query::par_chunks`].
///
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Query::par_chunks`]: crate::ecs::query::Query::par_chunks
pub struct QueryReadOnlyChunk<'b, 'q, Q: QueryData, F: QueryFilter> {
    start_idx: usize,
    end_idx: usize,
    fetch: &'b Q::Fetch<'q>,
    filter_data: &'b F::FilterData<'q>,
}

impl<'b, 'q, Q: QueryData, F: QueryFilter> QueryReadOnlyChunk<'b, 'q, Q, F> {
    /// Returns an [`Iterator`] over the matching read-only query items in this chunk.
    ///
    /// [`Iterator`]: std::iter::Iterator
    pub fn iter<'d>(&'d self) -> impl Iterator<Item = Q::ReadOnlyItem<'d>> {
        let fetch = self.fetch;
        let filter_data = self.filter_data;
        (self.start_idx..self.end_idx)
            .filter(move |&index| F::matches_row(filter_data, index))
            .map(move |index| unsafe { Q::fetch_read_only(fetch, index) })
    }
}

/// A mutable slice of rows within a single matched [`Archetype`], yielded by
/// [`Query::par_chunks_mut`].
///
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Query::par_chunks_mut`]: crate::ecs::query::Query::par_chunks_mut
pub struct QueryMutableChunk<'b, 'q, Q: QueryData, F: QueryFilter> {
    start_idx: usize,
    end_idx: usize,
    fetch: &'b Q::Fetch<'q>,
    filter_data: &'b F::FilterData<'q>,
}

impl<'b, 'q, Q: QueryData, F: QueryFilter> QueryMutableChunk<'b, 'q, Q, F> {
    /// Returns an [`Iterator`] over the matching read-only query items in this chunk.
    ///
    /// [`Iterator`]: std::iter::Iterator
    pub fn iter<'c>(&'c self) -> impl Iterator<Item = Q::ReadOnlyItem<'c>> {
        let fetch = self.fetch;
        let filter_data = self.filter_data;
        (self.start_idx..self.end_idx)
            .filter(move |&index| F::matches_row(filter_data, index))
            .map(move |index| unsafe { Q::fetch_read_only(fetch, index) })
    }

    /// Returns an [`Iterator`] over the matching mutable query items in this chunk.
    ///
    /// [`Iterator`]: std::iter::Iterator
    pub fn iter_mut<'c>(&'c mut self) -> impl Iterator<Item = Q::Item<'c>> {
        let fetch = self.fetch;
        let filter_data = self.filter_data;
        (self.start_idx..self.end_idx)
            .filter(move |&index| F::matches_row(filter_data, index))
            .map(move |index| unsafe { Q::fetch_mut(fetch, index) })
    }
}

/// Cached row count and [`QueryFilter`] state for a matched [`Archetype`].
///
/// [`QueryFilter`]: crate::ecs::query::filter::QueryFilter
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
pub struct MatchedArchetypeData<'q, F: QueryFilter> {
    row_count: usize,
    filter_data: F::FilterData<'q>,
}

impl<'q, F: QueryFilter> MatchedArchetypeData<'q, F> {
    /// Returns the number of entity rows stored in the matched [`Archetype`].
    ///
    /// [`Archetype`]: crate::ecs::world::archetypes::Archetype
    #[inline(always)]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Returns a shared reference to the initialized [`QueryFilter::FilterData`] for this
    /// archetype.
    ///
    /// [`QueryFilter::FilterData`]: crate::ecs::query::filter::QueryFilter::FilterData
    #[inline(always)]
    pub fn filter_data(&self) -> &F::FilterData<'q> {
        &self.filter_data
    }
}

/// A [`SystemParam`] that provides selective read-only or mutable access to components matching
/// [`QueryData`] `Q` and [`QueryFilter`] `F`.
///
/// `Query` is the primary way systems read and mutate entity components stored across matching
/// [`Archetype`] tables.
///
/// # Basic Usage
/// Pass a tuple (or single item) of [`QueryData`] as the first type parameter `Q`:
/// - `&T` grants shared read-only access to component `T`.
/// - `&mut T` grants exclusive mutable access via [`Mut<'_, T>`], automatically marking `T` as
///   changed when mutably dereferenced (if change tracking is enabled for `T`).
/// - `Option<&T>` and `Option<&mut T>` match entities whether or not they have component `T`.
/// - [`Entity`] yields the entity's handle.
///
/// ```rust
/// # use avilex::prelude::*;
/// #[derive(Component)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(Component)]
/// struct Name {
///     name: String
/// }
///
/// #[derive(Component)]
/// struct Velocity { x: f32, y: f32 }
///
/// fn move_entities(mut query: Query<(&mut Position, &Velocity, Option<&Name>)>) {
///     for (mut pos, vel, name) in query.iter_mut() {
///         pos.x += vel.x;
///         pos.y += vel.y;
///     }
/// }
/// ```
///
/// # Filtering Entities
/// Use the optional second type parameter `F: QueryFilter` to narrow which entities match without
/// fetching their component data:
/// - [`With<T>`]: Matches only entities that have component `T`.
/// - [`Without<T>`]: Matches only entities that do **not** have component `T`.
/// - [`Or<(F1, F2, ...)>`]: Matches entities that satisfy at least one filter in the tuple.
/// - [`Not<F>`]: Inverts filter `F`.
/// - Tuples `(F1, F2, ...)` require all filters to match (logical AND).
///
/// ```rust
/// # use avilex::prelude::*;
/// # #[derive(Component)] struct Position { x: f32, y: f32 }
/// # #[derive(Component)] struct Player;
/// # #[derive(Component)] struct Frozen;
/// fn move_active_players(mut query: Query<&mut Position, (With<Player>, Without<Frozen>)>) {
///     for mut pos in query.iter_mut() {
///         pos.x += 1.0;
///     }
/// }
/// ```
///
/// # Reactive Queries (`Added` & `Changed`)
/// Avilex supports reactive filters and trackers that detect component additions and mutations
/// from the previous frame (see [`reactivity`] for the 3-frame buffer lifecycle):
/// - [`Changed<T>`] / [`Added<T>`]: Used in the filter position `F` to yield only entities whose
///   component `T` was mutated or added in the previous frame.
/// - [`ChangedRefMut<T>`] / [`AddedTracker<T>`]: Used in the data position `Q` to inspect
///   `.is_changed()` or `.is_added()` per entity without filtering out unchanged rows.
///
/// ```rust
/// # use avilex::prelude::*;
/// # #[derive(Component)] struct Health(i32);
/// fn on_health_changed(query: Query<(Entity, &Health), Changed<Health>>) {
///     for (entity, health) in query.iter() {
///         println!("{entity:?} health changed to {}", health.0);
///     }
/// }
/// ```
///
/// # Single-Entity & Direct Lookups
/// - Use [`Query::single`] / [`Query::single_mut`] (or non-panicking [`Query::get_single`] /
///   [`Query::get_single_mut`]) when expecting exactly one matching entity, or use [`Single`] /
///   [`SingleMut`] directly as system parameters.
/// - Use [`Query::get`] / [`Query::get_mut`] to look up a specific [`Entity`] directly by handle.
///
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`QueryData`]: crate::ecs::query::QueryData
/// [`QueryFilter`]: crate::ecs::query::filter::QueryFilter
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Mut<'_, T>`]: crate::ecs::reactivity::changed::Mut
/// [`Entity`]: crate::ecs::entity::Entity
/// [`With<T>`]: crate::ecs::query::filter::With
/// [`Without<T>`]: crate::ecs::query::filter::Without
/// [`Or<(F1, F2, ...)>`]: crate::ecs::query::filter::Or
/// [`Not<F>`]: crate::ecs::query::filter::Not
/// [`reactivity`]: crate::ecs::reactivity
/// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
/// [`Added<T>`]: crate::ecs::reactivity::added::Added
/// [`ChangedRefMut<T>`]: crate::ecs::reactivity::changed::ChangedRefMut
/// [`AddedTracker<T>`]: crate::ecs::reactivity::added::AddedTracker
/// [`Query::single`]: Self::single
/// [`Query::single_mut`]: Self::single_mut
/// [`Query::get_single`]: Self::get_single
/// [`Query::get_single_mut`]: Self::get_single_mut
/// [`Single`]: crate::ecs::query::single::Single
/// [`SingleMut`]: crate::ecs::query::single::SingleMut
/// [`Query::get`]: Self::get
/// [`Query::get_mut`]: Self::get_mut
pub struct Query<'q, Q: QueryData, F: QueryFilter = ()> {
    cached_fetches: Vec<Option<Q::Fetch<'q>>>,
    cached_archetypes: Vec<Option<MatchedArchetypeData<'q, F>>>,
    entity_location_registry_guard: RwLockReadGuard<'q, Vec<EntityLocationData>>,
}

unsafe impl<'q, Q: QueryData, F: QueryFilter> Sync for Query<'q, Q, F> {}

impl<'q, Q: QueryData, F: QueryFilter> Query<'q, Q, F> {
    pub(crate) fn new(
        archetypes_map: &Arc<DashMap<ArchetypeId, Mutex<Archetype>, FxBuildHasher>>,
    ) -> Self {
        let len = archetypes_map.len();

        let mut cached_fetches = (0..len)
            .map(|_| None)
            .collect::<Vec<Option<Q::Fetch<'q>>>>();

        let mut cached_archetypes = (0..len)
            .map(|_| None)
            .collect::<Vec<Option<MatchedArchetypeData<'q, F>>>>();

        for arch_mutex in archetypes_map.iter() {
            let arch = arch_mutex.lock();
            let arch_id = arch.id() as usize;

            if Q::matches(&arch) && F::matches(&arch) {
                let fetch = unsafe {
                    std::mem::transmute::<<Q as QueryData>::Fetch<'_>, <Q as QueryData>::Fetch<'_>>(
                        Q::init_fetch(&arch),
                    )
                };

                let filter_data = F::init_filter_data(&arch);
                let row_count = arch.entities.read().len();

                cached_fetches[arch_id] = Some(fetch);
                cached_archetypes[arch_id] = Some(MatchedArchetypeData {
                    row_count,
                    filter_data,
                });
            }
        }

        let entity_location_registry_guard = unsafe {
            transmute::<
                RwLockReadGuard<'_, Vec<EntityLocationData>>,
                RwLockReadGuard<'q, Vec<EntityLocationData>>,
            >(ENTITY_LOCATION_REGISTRY.read())
        };

        Self {
            cached_fetches,
            cached_archetypes,
            entity_location_registry_guard,
        }
    }

    /// Returns the single matching read-only query item if exactly one entity matches `Q` and `F`.
    ///
    /// Returns `Err(`[`QuerySingleError::NoMatches`]`)` if zero entities match, or
    /// `Err(`[`QuerySingleError::MultipleMatches`]`)` if more than one entity matches.
    ///
    /// [`QuerySingleError::NoMatches`]: crate::ecs::query::single::QuerySingleError::NoMatches
    /// [`QuerySingleError::MultipleMatches`]: crate::ecs::query::single::QuerySingleError::MultipleMatches
    pub fn get_single<'w>(&'w self) -> Result<Q::ReadOnlyItem<'w>, QuerySingleError> {
        let mut target_arch_id: Option<usize> = None;
        let mut target_row_idx: Option<usize> = None;
        let mut match_count: usize = 0;

        for (arch_id, maybe_arch) in self.cached_archetypes.iter().enumerate() {
            if let Some(arch) = maybe_arch {
                if arch.row_count == 0 {
                    continue;
                }

                for row_idx in 0..arch.row_count {
                    if F::matches_row(&arch.filter_data, row_idx) {
                        match_count += 1;

                        if match_count > 1 {
                            return Err(QuerySingleError::MultipleMatches);
                        }

                        target_arch_id = Some(arch_id);
                        target_row_idx = Some(row_idx);
                    }
                }
            }
        }

        let arch_id = target_arch_id.ok_or(QuerySingleError::NoMatches)?;
        let row_idx = target_row_idx.ok_or(QuerySingleError::NoMatches)?;

        let fetch_state = self.cached_fetches[arch_id]
            .as_ref()
            .ok_or(QuerySingleError::NoMatches)?;
        unsafe { Ok(Q::fetch_read_only(fetch_state, row_idx)) }
    }

    /// Returns the single matching mutable query item if exactly one entity matches `Q` and `F`.
    ///
    /// Returns `Err(`[`QuerySingleError::NoMatches`]`)` if zero entities match, or
    /// `Err(`[`QuerySingleError::MultipleMatches`]`)` if more than one entity matches.
    ///
    /// [`QuerySingleError::NoMatches`]: crate::ecs::query::single::QuerySingleError::NoMatches
    /// [`QuerySingleError::MultipleMatches`]: crate::ecs::query::single::QuerySingleError::MultipleMatches
    pub fn get_single_mut<'w>(&'w mut self) -> Result<Q::Item<'w>, QuerySingleError> {
        let mut target_arch_id: Option<usize> = None;
        let mut target_row_idx: Option<usize> = None;
        let mut match_count: usize = 0;

        for (arch_id, maybe_arch) in self.cached_archetypes.iter().enumerate() {
            if let Some(arch) = maybe_arch {
                if arch.row_count == 0 {
                    continue;
                }

                for row_idx in 0..arch.row_count {
                    if F::matches_row(&arch.filter_data, row_idx) {
                        match_count += 1;

                        if match_count > 1 {
                            return Err(QuerySingleError::MultipleMatches);
                        }

                        target_arch_id = Some(arch_id);
                        target_row_idx = Some(row_idx);
                    }
                }
            }
        }

        let arch_id = target_arch_id.ok_or(QuerySingleError::NoMatches)?;
        let row_idx = target_row_idx.ok_or(QuerySingleError::NoMatches)?;

        let fetch_state = self.cached_fetches[arch_id]
            .as_ref()
            .ok_or(QuerySingleError::NoMatches)?;
        unsafe { Ok(Q::fetch_mut(fetch_state, row_idx)) }
    }

    /// Returns the single matching read-only query item.
    ///
    /// # Panics
    /// Panics if zero or more than one entity matches `Q` and `F`. Use [`Query::get_single`] for
    /// a non-panicking alternative.
    ///
    /// [`Query::get_single`]: Self::get_single
    pub fn single<'w>(&'w self) -> Q::ReadOnlyItem<'w> {
        self.get_single().unwrap_or_else(|err| {
            panic!("{}", err);
        })
    }

    /// Returns the single matching mutable query item.
    ///
    /// # Panics
    /// Panics if zero or more than one entity matches `Q` and `F`. Use [`Query::get_single_mut`]
    /// for a non-panicking alternative.
    ///
    /// [`Query::get_single_mut`]: Self::get_single_mut
    pub fn single_mut<'w>(&'w mut self) -> Q::Item<'w> {
        self.get_single_mut().unwrap_or_else(|err| {
            panic!("{}", err);
        })
    }

    /// Returns the total number of entity indices across all matching archetypes,
    /// fully accounting for any dynamic filters.
    pub fn filtered_entities(&self) -> usize {
        let mut count = 0;
        for arch in self.cached_archetypes.iter().flatten() {
            if arch.row_count == 0 {
                continue;
            }
            for row_idx in 0..arch.row_count {
                if F::matches_row(&arch.filter_data, row_idx) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Returns the total number of entities across all matched archetypes,
    /// without accounting for dynamic filters.
    pub fn base_entities(&self) -> usize {
        self.cached_archetypes
            .iter()
            .flatten()
            .map(|arch| arch.row_count)
            .sum()
    }

    /// Returns an [`Iterator`] over all matching read-only query items.
    ///
    /// [`Iterator`]: std::iter::Iterator
    pub fn iter<'b>(&'b self) -> impl Iterator<Item = Q::ReadOnlyItem<'b>> {
        self.cached_archetypes
            .iter()
            .enumerate()
            .filter_map(|(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = self.cached_fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(|(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .filter(move |&row_idx| F::matches_row(filter_data, row_idx))
                    .map(move |row_idx| unsafe { Q::fetch_read_only(fetch, row_idx) })
            })
    }

    /// Returns an [`Iterator`] over all matching mutable query items.
    ///
    /// [`Iterator`]: std::iter::Iterator
    pub fn iter_mut<'b>(&'b mut self) -> impl Iterator<Item = Q::Item<'b>> {
        let fetches = &self.cached_fetches;
        self.cached_archetypes
            .iter()
            .enumerate()
            .filter_map(move |(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(|(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .filter(move |&row_idx| F::matches_row(filter_data, row_idx))
                    .map(move |row_idx| unsafe { Q::fetch_mut(fetch, row_idx) })
            })
    }

    /// Returns a Rayon [`ParallelIterator`] over all matching read-only query items.
    ///
    /// [`ParallelIterator`]: rayon::iter::ParallelIterator
    pub fn par_iter<'b>(&'b self) -> impl ParallelIterator<Item = Q::ReadOnlyItem<'b>> {
        self.cached_archetypes
            .par_iter()
            .enumerate()
            .filter_map(|(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = self.cached_fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(|(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .into_par_iter()
                    .filter(move |&row_idx| F::matches_row(filter_data, row_idx))
                    .map(move |row_idx| unsafe { Q::fetch_read_only(fetch, row_idx) })
            })
    }

    /// Returns a Rayon [`ParallelIterator`] over all matching mutable query items.
    ///
    /// [`ParallelIterator`]: rayon::iter::ParallelIterator
    pub fn par_iter_mut<'b>(&'b mut self) -> impl ParallelIterator<Item = Q::Item<'b>> {
        let fetches = &self.cached_fetches;
        self.cached_archetypes
            .par_iter()
            .enumerate()
            .filter_map(move |(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(|(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .into_par_iter()
                    .filter(move |&row_idx| F::matches_row(filter_data, row_idx))
                    .map(move |row_idx| unsafe { Q::fetch_mut(fetch, row_idx) })
            })
    }

    /// Partitions matching archetype rows into chunks of up to `chunk_size` and yields a Rayon
    /// [`ParallelIterator`] of [`QueryReadOnlyChunk`] batches.
    ///
    /// # Panics
    /// Panics if `chunk_size == 0`.
    ///
    /// [`ParallelIterator`]: rayon::iter::ParallelIterator
    /// [`QueryReadOnlyChunk`]: crate::ecs::query::QueryReadOnlyChunk
    pub fn par_chunks<'b>(
        &'b self,
        chunk_size: usize,
    ) -> impl ParallelIterator<Item = QueryReadOnlyChunk<'b, 'q, Q, F>> {
        assert!(chunk_size > 0, "chunk_size must be non-zero");
        self.cached_archetypes
            .par_iter()
            .enumerate()
            .filter_map(|(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = self.cached_fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(move |(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .into_par_iter()
                    .step_by(chunk_size)
                    .map(move |start_idx| {
                        let end_idx = std::cmp::min(start_idx + chunk_size, total_rows);
                        QueryReadOnlyChunk {
                            start_idx,
                            end_idx,
                            fetch,
                            filter_data,
                        }
                    })
            })
    }

    /// Partitions matching archetype rows into chunks of up to `chunk_size` and yields a Rayon
    /// [`ParallelIterator`] of [`QueryMutableChunk`] batches.
    ///
    /// # Panics
    /// Panics if `chunk_size == 0`.
    ///
    /// [`ParallelIterator`]: rayon::iter::ParallelIterator
    /// [`QueryMutableChunk`]: crate::ecs::query::QueryMutableChunk
    pub fn par_chunks_mut<'b>(
        &'b mut self,
        chunk_size: usize,
    ) -> impl ParallelIterator<Item = QueryMutableChunk<'b, 'q, Q, F>> {
        assert!(chunk_size > 0, "chunk_size must be non-zero");
        let fetches = &self.cached_fetches;
        self.cached_archetypes
            .par_iter()
            .enumerate()
            .filter_map(move |(arch_id, maybe_arch)| {
                let arch = maybe_arch.as_ref()?;
                let fetch = fetches[arch_id].as_ref()?;
                Some((arch.row_count, fetch, &arch.filter_data))
            })
            .flat_map(move |(total_rows, fetch, filter_data)| {
                (0..total_rows)
                    .into_par_iter()
                    .step_by(chunk_size)
                    .map(move |start_idx| {
                        let end_idx = std::cmp::min(start_idx + chunk_size, total_rows);
                        QueryMutableChunk {
                            start_idx,
                            end_idx,
                            fetch,
                            filter_data,
                        }
                    })
            })
    }

    /// Performs an `O(1)` random-access lookup for `entity`, returning its read-only query item if
    /// its archetype matches `Q` and `F`.
    ///
    /// Note: This lookup checks archetype-level filters only and does not evaluate row-level
    /// dynamic filters such as [`Changed<T>`] or [`Added<T>`].
    ///
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    pub fn get<'w>(&'w self, entity: &Entity) -> Option<Q::ReadOnlyItem<'w>> {
        unsafe {
            let (arch_id, row_index) = self.get_row_and_arch(entity);

            let fetch = self.cached_fetches.get(arch_id.id() as usize)?.as_ref()?;
            Some(Q::fetch_read_only(fetch, row_index as usize))
        }
    }

    /// Performs an `O(1)` random-access lookup for `entity`, returning its mutable query item if
    /// its archetype matches `Q` and `F`.
    ///
    /// Note: This lookup checks archetype-level filters only and does not evaluate row-level
    /// dynamic filters such as [`Changed<T>`] or [`Added<T>`].
    ///
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    pub fn get_mut<'w>(&'w mut self, entity: &Entity) -> Option<Q::Item<'w>> {
        unsafe {
            let (arch_id, row_index) = self.get_row_and_arch(entity);

            let fetch = self.cached_fetches.get(arch_id.id() as usize)?.as_ref()?;
            Some(Q::fetch_mut(fetch, row_index as usize))
        }
    }

    /// Performs an unchecked `O(1)` random-access lookup for `entity`, returning its read-only
    /// query item without verifying that `entity` matches the query.
    ///
    /// Note: This lookup checks archetype-level filters only and does not evaluate row-level
    /// dynamic filters such as [`Changed<T>`] or [`Added<T>`].
    ///
    /// # Safety
    /// The caller must ensure that `entity` is currently alive and belongs to an archetype
    /// matched by this query.
    ///
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    pub unsafe fn get_unchecked<'w>(&'w self, entity: &Entity) -> Q::ReadOnlyItem<'w> {
        unsafe {
            let (arch_id, row_index) = self.get_row_and_arch(entity);
            let fetch = self
                .cached_fetches
                .get_unchecked(arch_id.id() as usize)
                .as_ref()
                .unwrap_unchecked();
            Q::fetch_read_only(fetch, row_index as usize)
        }
    }

    /// Performs an unchecked `O(1)` random-access lookup for `entity`, returning its mutable
    /// query item without verifying that `entity` matches the query.
    ///
    /// Note: This lookup checks archetype-level filters only and does not evaluate row-level
    /// dynamic filters such as [`Changed<T>`] or [`Added<T>`].
    ///
    /// # Safety
    /// The caller must ensure that `entity` is currently alive and belongs to an archetype
    /// matched by this query.
    ///
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    pub unsafe fn get_mut_unchecked<'w>(&'w mut self, entity: &Entity) -> Q::Item<'w> {
        unsafe {
            let (arch_id, row_index) = self.get_row_and_arch(entity);
            let fetch = self.cached_fetches[arch_id.id() as usize]
                .as_ref()
                .unwrap_unchecked();
            Q::fetch_mut(fetch, row_index as usize)
        }
    }

    fn get_row_and_arch(&self, entity: &Entity) -> (ArchetypeId, u32) {
        unsafe {
            let entity_location_data_ptr = self
                .entity_location_registry_guard
                .as_ptr()
                .add(entity.registry_index() as usize);
            let arch_id = (*entity_location_data_ptr).archetype_id();
            let row_index = (*entity_location_data_ptr).index();
            (arch_id, row_index)
        }
    }

    /// Returns a raw slice of the cached fetch states for each archetype.
    ///
    /// This provides direct access to the query's interior state tracking array. It allows
    /// low-level frameworks or custom batch runners to bypass standard iteration loops.
    ///
    /// # Safety
    /// Calling this method is safe, but interacting with the slice contents is highly unsafe.
    /// The caller must guarantee the following invariants:
    ///
    /// * **Aliasing Rules:** You must never invoke [`Self::fetch_mut`] concurrently on the same
    ///   row index, or across overlapping rows from multiple threads.
    /// * **Structural Modification:** The cached fetch tokens are only valid for the current frame.
    ///   Any structural change (spawning, despawning, component insertions/removals) instantly
    ///   invalidates these tokens.
    /// * **Initialization Check:** You must explicitly check if the target `Option` is `Some`
    ///   before unwrapping. Archetypes that do not match this query's filters will be `None`.
    pub unsafe fn get_fetches(&self) -> &[Option<Q::Fetch<'q>>] {
        &self.cached_fetches
    }

    /// Returns a raw slice of the cached matched archetype metadata records.
    ///
    /// Provides low-level frameworks and custom batch runners with direct access to the
    /// structural row bounds and dynamic filter states unified inside a single array.
    ///
    /// # Safety
    /// Bypasses standard query safety boundaries entirely. The caller must guarantee:
    ///
    /// * **State Lifetime Invalidation:** The underlying filter records inside the matching configurations
    ///   are transient structures valid only for the immediate frame context. You must never hold,
    ///   cache, or propagate these references beyond the boundary of the current execution frame.
    #[inline(always)]
    pub unsafe fn get_matched_archetypes_metadata(&self) -> &[Option<MatchedArchetypeData<'q, F>>] {
        &self.cached_archetypes
    }

    /// Unsafely evaluates whether a specific row index within an archetype matches the query's filters.
    ///
    /// # Safety
    /// * The `filter_data` reference must be valid, initialized, and sourced directly from a matched element inside [`Self::get_matched_archetypes_metadata`].
    /// * The `index` must be strictly less than the corresponding `row_count` exposed by that archetype's record.
    #[inline(always)]
    pub unsafe fn matches_row(&self, filter_data: &F::FilterData<'q>, index: usize) -> bool {
        F::matches_row(filter_data, index)
    }

    /// Unsafely extracts an immutable item view from the provided fetch token at the specified dense row index.
    ///
    /// # Safety
    /// * The `fetch` state must be valid, initialized (`Some`), and sourced directly from [`Self::get_fetches`].
    /// * The `index` must be strictly less than the length of the target archetype's data rows.
    /// * No mutable references ([`Self::fetch_mut`]) can be actively held for this row index on any thread.
    #[inline(always)]
    pub unsafe fn fetch_read_only<'w>(
        &'w self,
        fetch: &Q::Fetch<'w>,
        index: usize,
    ) -> Q::ReadOnlyItem<'w> {
        unsafe { Q::fetch_read_only(fetch, index) }
    }

    /// Unsafely extracts a mutable item view from the provided fetch token at the specified dense row index.
    ///
    /// # Safety
    /// * The `fetch` state must be valid, initialized (`Some`), and sourced directly from [`Self::get_fetches`].
    /// * The `index` must be strictly less than the length of the target archetype's data rows.
    /// * **Strict Aliasing Rule:** You must guarantee absolute exclusive access to this row. No other
    ///   thread or task may hold a read-only or mutable reference to this exact element simultaneously.
    #[inline(always)]
    pub unsafe fn fetch_mut<'w>(&'w self, fetch: &Q::Fetch<'w>, index: usize) -> Q::Item<'w> {
        unsafe { Q::fetch_mut(fetch, index) }
    }
}

impl<'q, Q: QueryData, F: QueryFilter> SystemParam for Query<'q, Q, F> {
    type Item<'world> = Query<'world, Q, F>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        let mut local_reads = AccessVec::new();
        let mut local_writes = AccessVec::new();
        let mut local_with = AccessVec::new();
        let mut local_without = AccessVec::new();

        Q::collect_access(&mut local_reads, &mut local_writes);
        F::collect_filter_access(&mut local_reads, &mut local_writes);
        F::collect_filter(&mut local_with, &mut local_without);

        let has_intra_conflict = local_writes.iter().any(|w| local_reads.contains(w));
        let mut unique_writes = FxHashSet::default();
        let has_duplicate_writes = local_writes.iter().any(|w| !unique_writes.insert(w));

        if has_intra_conflict || has_duplicate_writes {
            panic!(
                "QueryAccessError\n\
                Type: IntraQueryConflict\n\
                System: {}\n\
                Query: {}",
                system_meta.system_name(),
                type_name::<Self>()
            );
        }
        let mut has_inter_conflict = false;

        for id in local_writes.iter() {
            if system_meta.has_component_write(id) || system_meta.has_component_read(id) {
                has_inter_conflict = true;
                break;
            }
        }
        if !has_inter_conflict {
            for id in local_reads.iter() {
                if system_meta.has_component_write(id) {
                    has_inter_conflict = true;
                    break;
                }
            }
        }

        if has_inter_conflict {
            let is_disjoint = local_without
                .iter()
                .any(|wo| system_meta.has_with_filter(wo))
                || local_with.iter().any(|w| system_meta.has_without_filter(w));

            if !is_disjoint {
                panic!(
                    "QueryAccessError\n\
                    Type: SystemArgumentConflict\n\
                    System: {}\n\
                    Query: {}",
                    system_meta.system_name(),
                    type_name::<Self>()
                );
            }
        }

        for id in local_reads.iter().copied() {
            system_meta.add_component_read(id);
        }
        for id in local_writes.iter().copied() {
            system_meta.add_component_write(id);
        }
        for id in local_with.iter().copied() {
            system_meta.add_with_filter(id);
        }
        for id in local_without.iter().copied() {
            system_meta.add_without_filter(id);
        }
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        let query = Query::<Q, F>::new(&world.archetypes_manager.archetypes_map());
        Ok(query)
    }
}
