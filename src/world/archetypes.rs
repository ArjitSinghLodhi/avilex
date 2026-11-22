use std::{
    any::{Any, TypeId, type_name},
    hash::{BuildHasher, Hash},
    mem::transmute,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use dashmap::{
    DashMap,
    mapref::{multiple::RefMulti, one::Ref},
};
use indexmap::{IndexMap, IndexSet};
use parking_lot::{
    ArcRwLockReadGuard, Mutex, MutexGuard, RawRwLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::{
    component::bundle::ComponentBundle, ecs::component::Component, entity::Entity,
    reactivity::TRACKED_COMPONENTS,
};

pub(crate) trait AnyColumn: Any {
    unsafe fn swap_remove_erased(&mut self, idx: usize);
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn as_any(&self) -> &dyn Any;
    unsafe fn move_row_erased(&mut self, index: usize, dst: *mut dyn AnyColumn);
    fn clone_empty(&self) -> Box<dyn AnyColumn>;
}

impl<T: Component> AnyColumn for Vec<T> {
    unsafe fn swap_remove_erased(&mut self, idx: usize) {
        self.swap_remove(idx);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    unsafe fn move_row_erased(&mut self, index: usize, dst: *mut dyn AnyColumn) {
        let item = self.swap_remove(index);

        let dst_vec = unsafe {
            (&mut *dst)
                .as_any_mut()
                .downcast_mut::<Vec<T>>()
                .expect("Archetype component column type mismatch")
        };

        dst_vec.push(item);
    }

    fn clone_empty(&self) -> Box<dyn AnyColumn> {
        Box::new(Vec::<T>::new())
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
pub(crate) struct ArchetypeId(u32);

impl ArchetypeId {
    pub(crate) fn new(id: u32) -> ArchetypeId {
        ArchetypeId(id)
    }
    pub(crate) fn id(&self) -> u32 {
        self.0
    }
}

/// A type-erased, thread-safe storage column for a single [`Component`] type within an [`Archetype`].
///
/// Internally wraps a heap-allocated `Vec<T>` behind an [`RwLock`], allowing multiple systems
/// or parallel workers to read component arrays concurrently while guaranteeing exclusive access
/// during writes or structural mutations.
///
/// [`Component`]: crate::ecs::component::Component
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`RwLock`]: parking_lot::RwLock
pub struct ComponentColumn {
    pub(crate) data: RwLock<Box<dyn AnyColumn>>,
}

unsafe impl Send for ComponentColumn {}
unsafe impl Sync for ComponentColumn {}

impl ComponentColumn {
    #[doc(hidden)]
    #[allow(private_bounds)]
    pub fn new<T: AnyColumn>(column: T) -> Self {
        Self {
            data: RwLock::new(Box::new(column)),
        }
    }
}

/// A self-contained, `'static` read-only RAII guard providing direct, zero-overhead access
/// to a typed component column (`Vec<T>`).
///
/// Created via [`Archetype::get_column`] or [`Archetype::column`].
///
/// # Self-Contained Ownership
/// Internally retains an [`Arc`] reference to the archetype's column map, ensuring the underlying
/// [`RwLock`] and `Vec<T>` remain valid and alive for the entire lifetime of this guard—even if
/// the parent [`Archetype`] borrow ends, is moved, or is dropped, or if this guard is sent across
/// thread boundaries.
///
/// # Synchronization
/// While held, this guard maintains a shared read lock on the underlying [`ComponentColumn`],
/// blocking structural mutations or mutable borrows ([`ComponentColumnWrite`]) of the same
/// column across threads.
///
/// Implements [`Deref<Target = Vec<T>>`] for direct slice and index access.
///
/// [`Component`]: crate::ecs::component::Component
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Archetype::get_column`]: crate::ecs::world::archetypes::Archetype::get_column
/// [`Archetype::column`]: crate::ecs::world::archetypes::Archetype::column
/// [`ComponentColumn`]: crate::ecs::world::archetypes::ComponentColumn
/// [`ComponentColumnWrite`]: crate::ecs::world::archetypes::ComponentColumnWrite
/// [`Arc`]: std::sync::Arc
/// [`RwLock`]: parking_lot::RwLock
/// [`Deref<Target = Vec<T>>`]: std::ops::Deref
pub struct ComponentColumnRead<T: Component> {
    column: *const Vec<T>,
    _guard: RwLockReadGuard<'static, Box<dyn AnyColumn>>,
    _arc_clone: Arc<IndexMap<TypeId, ComponentColumn, FxBuildHasher>>,
}

unsafe impl<T: Component> Send for ComponentColumnRead<T> {}
unsafe impl<T: Component> Sync for ComponentColumnRead<T> {}

impl<T: Component> Deref for ComponentColumnRead<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.column) }
    }
}

/// A self-contained, `'static` exclusive read-write RAII guard providing direct, zero-overhead
/// mutable access to a typed component column (`Vec<T>`).
///
/// Created via [`Archetype::get_column_mut`] or [`Archetype::column_mut`].
///
/// # Self-Contained Ownership
/// Internally retains an [`Arc`] reference to the archetype's column map, ensuring the underlying
/// [`RwLock`] and `Vec<T>` remain valid and alive for the entire lifetime of this guard—even if
/// the parent [`Archetype`] borrow ends, is moved, or is dropped, or if this guard is sent across
/// thread boundaries.
///
/// # Synchronization
/// While held, this guard maintains an exclusive write lock on the underlying [`ComponentColumn`],
/// ensuring safe parallel iteration and mutation without data races.
///
/// Implements [`Deref<Target = Vec<T>>`] and [`DerefMut`].
///
/// [`Component`]: crate::ecs::component::Component
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Archetype::get_column_mut`]: crate::ecs::world::archetypes::Archetype::get_column_mut
/// [`Archetype::column_mut`]: crate::ecs::world::archetypes::Archetype::column_mut
/// [`ComponentColumn`]: crate::ecs::world::archetypes::ComponentColumn
/// [`Arc`]: std::sync::Arc
/// [`RwLock`]: parking_lot::RwLock
/// [`Deref<Target = Vec<T>>`]: std::ops::Deref
/// [`DerefMut`]: std::ops::DerefMut
pub struct ComponentColumnWrite<T: Component> {
    column: *mut Vec<T>,
    _guard: RwLockWriteGuard<'static, Box<dyn AnyColumn>>,
    _arc_clone: Arc<IndexMap<TypeId, ComponentColumn, FxBuildHasher>>,
}

unsafe impl<T: Component> Send for ComponentColumnWrite<T> {}
unsafe impl<T: Component> Sync for ComponentColumnWrite<T> {}

impl<T: Component> Deref for ComponentColumnWrite<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.column) }
    }
}

impl<T: Component> DerefMut for ComponentColumnWrite<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut (*self.column) }
    }
}

/// A self-contained, `'static` read-only RAII guard providing access to the dense [`Entity`]
/// table of an [`Archetype`].
///
/// Created via [`Archetype::entities`]. Each index `i` in this vector corresponds directly to
/// row `i` across every [`ComponentColumn`] stored in the same archetype.
///
/// # Self-Contained Ownership & Synchronization
/// Backed by an [`ArcRwLockReadGuard`], this guard retains shared ownership of the entity table's
/// [`Arc<RwLock<Vec<Entity>>>`], allowing it to be held, stored, or sent across threads
/// independently of the parent [`Archetype`] borrow while blocking structural entity mutations
/// on that archetype until dropped.
///
/// Implements [`Deref<Target = Vec<Entity>>`] for direct slice and index access.
///
/// [`Entity`]: crate::ecs::entity::Entity
/// [`Archetype`]: crate::ecs::world::archetypes::Archetype
/// [`Archetype::entities`]: crate::ecs::world::archetypes::Archetype::entities
/// [`ComponentColumn`]: crate::ecs::world::archetypes::ComponentColumn
/// [`ArcRwLockReadGuard`]: parking_lot::lock_api::ArcRwLockReadGuard
/// [`Arc<RwLock<Vec<Entity>>>`]: std::sync::Arc
/// [`Deref<Target = Vec<Entity>>`]: std::ops::Deref
pub struct EntitiesRead {
    guard: ArcRwLockReadGuard<RawRwLock, Vec<Entity>>,
}

impl Deref for EntitiesRead {
    type Target = Vec<Entity>;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// A dense, table-based storage container grouping all entities that share the exact same set of
/// [`Component`] types.
///
/// # Memory Layout & Row Alignment
/// An `Archetype` stores entities and their components in parallel, tightly packed contiguous
/// arrays (`Vec<T>`) indexed by row:
/// - Row `i` in [`Archetype::entities`] corresponds 1:1 to element `i` in every [`ComponentColumn`].
/// - Storing identical component layouts together maximizes CPU cache-line saturation and
///   hardware prefetch efficiency during query iteration.
///
/// # Self-Contained Guards
/// All entity and column accessors ([`entities`](Self::entities), [`column`](Self::column),
/// [`column_mut`](Self::column_mut)) return `'static`, self-contained RAII guards backed by [`Arc`].
/// Callers can drop their borrow on `Archetype` immediately after acquiring a guard, store guards
/// in structs, or send them across threads safely.
///
/// # Deadlocks
/// Each component column is guarded by an independent [`RwLock`]. Acquiring a write guard
/// ([`column_mut`](Self::column_mut)) while holding any read or write guard for the **same**
/// component type `T` on the same thread will deadlock. Accessing **different** component
/// types (`T` and `U`) on the same archetype concurrently is safe.
///
/// [`Component`]: crate::ecs::component::Component
/// [`ComponentColumn`]: crate::ecs::world::archetypes::ComponentColumn
/// [`Arc`]: std::sync::Arc
/// [`RwLock`]: parking_lot::RwLock
pub struct Archetype {
    pub(crate) id: ArchetypeId,
    pub(crate) types: IndexSet<TypeId, FxBuildHasher>,
    pub(crate) entities: Arc<RwLock<Vec<Entity>>>,
    pub(crate) columns: Arc<IndexMap<TypeId, ComponentColumn, FxBuildHasher>>,
    pub(crate) type_names: IndexSet<&'static str, FxBuildHasher>,
}

unsafe impl Sync for Archetype {}
unsafe impl Send for Archetype {}

impl Archetype {
    pub(crate) fn new(
        id: ArchetypeId,
        types: IndexSet<TypeId, FxBuildHasher>,
        columns: IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
        type_names: IndexSet<&'static str, FxBuildHasher>,
    ) -> Self {
        Self {
            id,
            types,
            entities: Arc::new(RwLock::new(Vec::new())),
            columns: Arc::new(columns),
            type_names,
        }
    }

    /// Acquires a shared, self-contained read lock over the dense list of [`Entity`] handles
    /// stored in this archetype.
    ///
    /// The returned [`EntitiesRead`] guard is `'static` and dereferences to `&Vec<Entity>`,
    /// where each index aligns 1:1 with the row index in every component column of this archetype.
    ///
    /// [`Entity`]: crate::ecs::entity::Entity
    /// [`EntitiesRead`]: crate::ecs::world::archetypes::EntitiesRead
    #[inline(always)]
    pub fn entities(&self) -> EntitiesRead {
        let guard = self.entities.read_arc();
        EntitiesRead { guard }
    }

    /// Returns `true` if this archetype stores a column for component type `T`.
    ///
    /// [`Component`]: crate::ecs::component::Component
    #[inline]
    pub fn contains_column<T: Component>(&self) -> bool {
        self.types.contains(&TypeId::of::<T>())
    }

    /// Attempts to acquire a shared, self-contained read-only guard for the component column of type `T`.
    ///
    /// Returns `None` if this archetype does not contain component `T`.
    ///
    /// # Deadlocks & Blocking
    /// Blocks the current thread if another thread holds a mutable write lock
    /// ([`ComponentColumnWrite`]) on the same column. Calling this while the **same** thread
    /// already holds a write guard for `T` will deadlock.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`ComponentColumnRead`]: crate::ecs::world::archetypes::ComponentColumnRead
    /// [`ComponentColumnWrite`]: crate::ecs::world::archetypes::ComponentColumnWrite
    pub fn get_column<T: Component>(&self) -> Option<ComponentColumnRead<T>> {
        let col = self.columns.get(&TypeId::of::<T>())?;
        let read_guard = col.data.read();
        let vector = read_guard
            .as_any()
            .downcast_ref::<Vec<T>>()
            .expect("Type mismatch") as *const Vec<T>;

        // SAFETY: `_arc_clone` keeps `self.columns` (and `col.data`) alive until after `_guard` drops.
        let static_guard: RwLockReadGuard<'static, Box<dyn AnyColumn>> =
            unsafe { transmute(read_guard) };

        Some(ComponentColumnRead {
            column: vector,
            _guard: static_guard,
            _arc_clone: Arc::clone(&self.columns),
        })
    }

    /// Acquires a shared, self-contained read-only guard for the component column of type `T`.
    ///
    /// # Panics
    /// Panics with `ColumnMissingError` if this archetype does not contain component `T`.
    /// Use [`Archetype::get_column`] for a non-panicking alternative.
    ///
    /// # Deadlocks & Blocking
    /// Blocks if another thread holds a write lock on `T`, and deadlocks if the **same** thread
    /// already holds a [`ComponentColumnWrite`] guard for `T`.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`ComponentColumnRead`]: crate::ecs::world::archetypes::ComponentColumnRead
    /// [`ComponentColumnWrite`]: crate::ecs::world::archetypes::ComponentColumnWrite
    /// [`Archetype::get_column`]: crate::ecs::world::archetypes::Archetype::get_column
    pub fn column<T: Component>(&self) -> ComponentColumnRead<T> {
        self.get_column::<T>().unwrap_or_else(|| {
            panic!(
                "ColumnMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    /// Attempts to acquire an exclusive, self-contained mutable guard for the component column of type `T`.
    ///
    /// Returns `None` if this archetype does not contain component `T`.
    ///
    /// # Deadlocks & Blocking
    /// Blocks the current thread until all active readers ([`ComponentColumnRead`]) or any existing
    /// writer ([`ComponentColumnWrite`]) on this column release their locks. Calling this while
    /// the **same** thread holds any read or write guard for `T` will deadlock.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`ComponentColumnRead`]: crate::ecs::world::archetypes::ComponentColumnRead
    /// [`ComponentColumnWrite`]: crate::ecs::world::archetypes::ComponentColumnWrite
    pub fn get_column_mut<T: Component>(&self) -> Option<ComponentColumnWrite<T>> {
        let col = self.columns.get(&TypeId::of::<T>())?;
        let mut write_guard = col.data.write();
        let vector = write_guard
            .as_any_mut()
            .downcast_mut::<Vec<T>>()
            .expect("Type mismatch") as *mut Vec<T>;

        // SAFETY: `_arc_clone` keeps `self.columns` (and `col.data`) alive until after `_guard` drops.
        let static_guard: RwLockWriteGuard<'static, Box<dyn AnyColumn>> =
            unsafe { transmute(write_guard) };

        Some(ComponentColumnWrite {
            column: vector,
            _guard: static_guard,
            _arc_clone: Arc::clone(&self.columns),
        })
    }

    /// Acquires an exclusive, self-contained mutable guard for the component column of type `T`.
    ///
    /// # Panics
    /// Panics with `ColumnMissingError` if this archetype does not contain component `T`.
    /// Use [`Archetype::get_column_mut`] for a non-panicking alternative.
    ///
    /// # Deadlocks & Blocking
    /// Blocks until existing locks on `T` are released, and deadlocks if the **same** thread
    /// already holds any read or write guard for `T`.
    ///
    /// [`Component`]: crate::ecs::component::Component
    /// [`ComponentColumnRead`]: crate::ecs::world::archetypes::ComponentColumnRead
    /// [`ComponentColumnWrite`]: crate::ecs::world::archetypes::ComponentColumnWrite
    /// [`Archetype::get_column_mut`]: crate::ecs::world::archetypes::Archetype::get_column_mut
    pub fn column_mut<T: Component>(&self) -> ComponentColumnWrite<T> {
        self.get_column_mut::<T>().unwrap_or_else(|| {
            panic!(
                "ColumnMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    pub(crate) fn id(&self) -> u32 {
        self.id.0
    }
}

pub(crate) struct ArchetypeManager {
    index: FxHashMap<u64, ArchetypeId>,
    archetypes: Arc<DashMap<ArchetypeId, Mutex<Archetype>, FxBuildHasher>>,
    next_id: u32,
}

impl ArchetypeManager {
    pub(crate) fn new() -> Self {
        Self {
            index: FxHashMap::default(),
            archetypes: Arc::new(DashMap::with_hasher(FxBuildHasher)),
            next_id: 0,
        }
    }

    fn calculate_hash(&self, types: &IndexSet<TypeId, FxBuildHasher>) -> u64 {
        let mut combined_hash: u64 = 0;
        let hasher_builder = self.index.hasher();
        for type_id in types {
            combined_hash = combined_hash.wrapping_add(hasher_builder.hash_one(type_id));
        }
        combined_hash
    }

    pub(crate) fn sync_tracking_states(&self, types: &mut IndexSet<TypeId, FxBuildHasher>) {
        let tracked = TRACKED_COMPONENTS.lock();
        for (comp_id, meta_list) in tracked.iter() {
            if types.contains(comp_id) {
                for meta in meta_list {
                    types.insert(meta.marker_id);
                }
            } else {
                for meta in meta_list {
                    types.swap_remove(&meta.marker_id);
                }
            }
        }
    }

    pub(crate) fn find_target_id_for_addition(
        &self,
        old_id: ArchetypeId,
        incoming_ids: &[TypeId],
    ) -> Option<ArchetypeId> {
        let old_arch_mutex = self.archetypes.get(&old_id)?;
        let old_arch = old_arch_mutex.lock();
        let mut target_types = old_arch.types.clone();
        for id in incoming_ids {
            target_types.insert(*id);
        }

        self.sync_tracking_states(&mut target_types);

        let hash = self.calculate_hash(&target_types);
        self.index.get(&hash).copied()
    }

    pub(crate) fn find_target_id_for_subtraction(
        &self,
        old_id: ArchetypeId,
        removed_ids: &[TypeId],
    ) -> Option<ArchetypeId> {
        let old_arch_mutex = self.archetypes.get(&old_id)?;
        let old_arch = old_arch_mutex.lock();
        let mut target_types = old_arch.types.clone();
        for id in removed_ids {
            target_types.swap_remove(id);
        }

        self.sync_tracking_states(&mut target_types);

        let hash = self.calculate_hash(&target_types);
        self.index.get(&hash).copied()
    }

    pub(crate) fn get_or_create_from_set<F>(
        &mut self,
        types_set: IndexSet<TypeId, FxBuildHasher>,
        types_names_set: IndexSet<&'static str, FxBuildHasher>,
        setup_columns: F,
    ) -> ArchetypeId
    where
        F: FnOnce(
            &IndexSet<TypeId, FxBuildHasher>,
            &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
        ),
    {
        let order_independent_hash = self.calculate_hash(&types_set);
        if let Some(id) = self.index.get(&order_independent_hash).copied() {
            return id;
        }

        let new_id = ArchetypeId::new(self.next_id);
        self.next_id += 1;
        let mut columns = IndexMap::with_capacity_and_hasher(types_set.len(), FxBuildHasher);
        setup_columns(&types_set, &mut columns);

        let new_arch = Archetype::new(new_id, types_set, columns, types_names_set);

        self.index.insert(order_independent_hash, new_id);
        self.archetypes.insert(new_id, Mutex::new(new_arch));
        new_id
    }

    pub(crate) fn get_or_create_from_generic<T: ComponentBundle>(&mut self) -> ArchetypeId {
        let incoming_ids = T::get_type_ids();
        let mut types_set = IndexSet::with_capacity_and_hasher(incoming_ids.len(), FxBuildHasher);
        for &id in incoming_ids {
            types_set.insert(id);
        }

        self.sync_tracking_states(&mut types_set);

        let order_independent_hash = self.calculate_hash(&types_set);
        if let Some(id) = self.index.get(&order_independent_hash).copied() {
            return id;
        }

        let incoming_names = T::get_type_names();
        let names_ref = incoming_names.as_ref();
        let mut types_names_set =
            IndexSet::with_capacity_and_hasher(names_ref.len(), FxBuildHasher);
        for &name in names_ref {
            types_names_set.insert(name);
        }

        let new_id = ArchetypeId(self.next_id);
        self.next_id += 1;

        let mut columns = IndexMap::with_hasher(FxBuildHasher);
        T::create_empty_columns(&mut columns);

        let tracked = TRACKED_COMPONENTS.lock();
        for &comp_id in incoming_ids {
            if let Some(meta_list) = tracked.get(&comp_id) {
                for meta in meta_list {
                    if types_set.contains(&meta.marker_id) {
                        columns.insert(meta.marker_id, (meta.create_marker_column)());
                    }
                }
            }
        }
        let new_arch = Archetype::new(new_id, types_set, columns, types_names_set);
        self.index.insert(order_independent_hash, new_id);
        self.archetypes.insert(new_id, Mutex::new(new_arch));
        new_id
    }

    pub(crate) fn iter_archetypes<'a>(
        &'a self,
    ) -> impl Iterator<Item = RefMulti<'a, ArchetypeId, Mutex<Archetype>>> {
        self.archetypes.iter()
    }

    pub(crate) fn archetypes_map(
        &self,
    ) -> Arc<DashMap<ArchetypeId, Mutex<Archetype>, FxBuildHasher>> {
        self.archetypes.clone()
    }

    pub(crate) fn get_mut<'a>(&'a self, archetype_id: ArchetypeId) -> Option<ArchetypeLock<'a>> {
        let ref_lock: Ref<'_, ArchetypeId, Mutex<Archetype>> =
            unsafe { transmute(self.archetypes.get(&archetype_id)?) };
        let mutex_lock: MutexGuard<'_, Archetype> = unsafe { transmute(ref_lock.lock()) };
        Some(ArchetypeLock {
            mutex_lock,
            _ref_lock: ref_lock,
        })
    }
}

pub(crate) struct ArchetypeLock<'a> {
    mutex_lock: MutexGuard<'a, Archetype>,
    _ref_lock: Ref<'a, ArchetypeId, Mutex<Archetype>>,
}

impl<'a> Deref for ArchetypeLock<'a> {
    type Target = Archetype;
    fn deref(&self) -> &Self::Target {
        &self.mutex_lock
    }
}

impl<'a> DerefMut for ArchetypeLock<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mutex_lock
    }
}
