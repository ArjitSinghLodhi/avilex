use crate::ecs::component::Component;
use crate::query::QueryData;
use crate::query::QueryFilter;
use crate::reactivity::verify_app_not_built_yet;
use crate::reactivity::{TRACKED_COMPONENTS, TrackedComponentMeta};
use crate::system::AccessVec;
use crate::world::archetypes::ComponentColumnRead;
use crate::world::archetypes::{Archetype, ComponentColumn};
use avilex_macros::Component;
use parking_lot::RwLock;
use std::any::TypeId;
use std::any::type_name;
use std::marker::PhantomData;

pub(crate) fn register_changed_tracked_component<T: Component>() {
    let mut tracked = TRACKED_COMPONENTS.lock();
    let component_id = TypeId::of::<T>();
    let marker_id = TypeId::of::<ChangedState<T>>();

    let meta_list = tracked.entry(component_id).or_default();

    if meta_list.iter().any(|m| m.marker_id == marker_id) {
        return;
    }
    let _lock = verify_app_not_built_yet(type_name::<ChangedState<T>>());
    meta_list.push(TrackedComponentMeta {
        marker_id,
        create_marker_column: || ComponentColumn {
            data: RwLock::new(Box::new(Vec::<ChangedState<T>>::new())),
        },
        push_default_marker: |column| {
            let mut guard = column.data.write();
            let raw_any = guard.as_any_mut();
            let vec = raw_any.downcast_mut::<Vec<ChangedState<T>>>().unwrap();
            vec.push(ChangedState {
                read: false,
                write: true,
                phantom: PhantomData,
            });
        },
        clear_column_states: |raw_any| {
            let vec = raw_any.downcast_mut::<Vec<ChangedState<T>>>().unwrap();
            vec.iter_mut().for_each(|state| {
                state.read = state.write;
                state.write = false;
            });
        },
    });
}

/// Double-buffered change detection state (`read` and `write` flags) for component `T`.
///
/// - `read` holds the visible change state for the current frame.
/// - `write` holds the staged change state that will be swapped into `read` (and reset to
///   `false`) at the next frame boundary.
///
/// [`Component`]: crate::ecs::component::Component
#[derive(Component, Clone, Copy, Debug)]
pub struct ChangedState<T: Component> {
    read: bool,
    write: bool,
    phantom: PhantomData<T>,
}

impl<T: Component> ChangedState<T> {
    /// Creates a new [`ChangedState`] with the given `read` and `write` flags.
    ///
    /// [`ChangedState`]: Self
    pub const fn new(read: bool, write: bool) -> Self {
        Self {
            read,
            write,
            phantom: PhantomData,
        }
    }

    /// Returns the `read` flag currently visible this frame.
    pub const fn read(&self) -> bool {
        self.read
    }

    /// Sets the `read` flag currently visible this frame.
    pub fn set_read(&mut self, read: bool) {
        self.read = read;
    }

    /// Returns the `write` flag staged to swap into `read` on the next frame.
    pub const fn write(&self) -> bool {
        self.write
    }

    /// Sets the `write` flag staged to swap into `read` on the next frame.
    pub fn set_write(&mut self, write: bool) {
        self.write = write;
    }
}

/// A query data wrapper that allows systems to inspect the change state of an individual component instance.
///
/// Unlike the [`Changed<T>`] filter, which excludes non-modified entities from the query entirely,
/// `ChangedTracker<T>` allows the entity to match the query while exposing the [`.is_changed()`] method
/// to check its status conditionally.
///
/// # Architecture & Timing
///
/// This tracking relies on the exact same double-buffered, frame-locked system as the filter.
/// For detailed breakdown of the 3-frame lifecycle timing, command application phases, and
/// initial spawn behavior, see the documentation for [`Changed<T>`].
///
/// [`.is_changed()`]: ChangedTracker::is_changed
pub struct ChangedTracker<T: Component> {
    changed: bool,
    _marker: PhantomData<T>,
}

impl<T: Component> ChangedTracker<T> {
    /// Returns `true` if component `T` was marked as changed during the previous frame.
    #[inline(always)]
    pub fn is_changed(&self) -> bool {
        self.changed
    }
}

impl<T: Component> QueryData for ChangedTracker<T> {
    type Item<'w> = ChangedTracker<T>;
    type ReadOnlyItem<'w> = ChangedTracker<T>;
    type Fetch = ComponentColumnRead<ChangedState<T>>;

    fn matches(archetype: &Archetype) -> bool {
        archetype.contains_column::<T>()
    }

    fn collect_access(
        reads: &mut AccessVec<std::any::TypeId>,
        _writes: &mut AccessVec<std::any::TypeId>,
    ) {
        reads.push(TypeId::of::<ChangedState<T>>());
        register_changed_tracked_component::<T>();
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        archetype.column::<ChangedState<T>>()
    }

    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        let marker_ref = unsafe { &(*fetch.as_ptr().add(index)) };
        let changed = marker_ref.read;
        ChangedTracker {
            changed,
            _marker: PhantomData,
        }
    }

    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        let marker_ref = unsafe { &(*fetch.as_ptr().add(index)) };
        let changed = marker_ref.read;
        ChangedTracker {
            changed,
            _marker: PhantomData,
        }
    }
}

/// A mutable [`QueryData`] item wrapping [`Mut<'w, T>`] that also exposes whether component `T`
/// changed during the previous frame.
///
/// Mutably dereferencing (`DerefMut`) automatically marks `T` as changed for the upcoming frame,
/// if change tracking is enabled for `T` (see [`reactivity`]). Use
/// [`ChangedRefMut::bypass_change_detection`] to mutate without triggering change detection.
///
/// [`QueryData`]: crate::ecs::query::QueryData
/// [`Mut<'w, T>`]: crate::ecs::reactivity::changed::Mut
/// [`reactivity`]: crate::ecs::reactivity
/// [`ChangedRefMut::bypass_change_detection`]: Self::bypass_change_detection
pub struct ChangedRefMut<'w, T: Component> {
    value: Mut<'w, T>,
    changed: bool,
}

/// A read-only wrapper around `&'w T` that also exposes whether component `T` changed during the
/// previous frame.
///
/// Yielded as the read-only item when iterating a [`Query`] over [`ChangedRefMut<'w, T>`]
/// immutably (for example via `query.iter()`). See [`reactivity`] for how change tracking is
/// enabled.
///
/// [`Query`]: crate::ecs::query::Query
/// [`ChangedRefMut<'w, T>`]: crate::ecs::reactivity::changed::ChangedRefMut
/// [`reactivity`]: crate::ecs::reactivity
pub struct ChangedRef<'w, T: Component> {
    value: &'w T,
    changed: bool,
}

impl<'w, T: Component> ChangedRefMut<'w, T> {
    /// Returns `true` if component `T` was marked as changed in the previous frame.
    #[inline(always)]
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    /// Returns a raw mutable reference (`&mut T`) without marking the component as changed.
    #[inline(always)]
    pub fn bypass_change_detection(&mut self) -> &mut T {
        self.value.bypass_change_detection()
    }

    /// Manually marks the component as changed by setting its staged [`ChangedState`] `write` flag.
    ///
    /// [`ChangedState`]: crate::ecs::reactivity::changed::ChangedState
    #[inline(always)]
    pub fn trigger_change_detection(&mut self) {
        self.value.trigger_change_detection();
    }
}

impl<'w, T: Component> ChangedRef<'w, T> {
    /// Returns `true` if component `T` was marked as changed in the previous frame.
    #[inline(always)]
    pub fn is_changed(&self) -> bool {
        self.changed
    }
}

impl<'w, T: Component> std::ops::Deref for ChangedRefMut<'w, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value.value }
    }
}

impl<'w, T: Component> std::ops::Deref for ChangedRef<'w, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'w, T: Component> std::ops::DerefMut for ChangedRefMut<'w, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.trigger_change_detection();
        unsafe { &mut *self.value.value }
    }
}

impl<'b, T: Component> QueryData for ChangedRefMut<'b, T> {
    type Item<'w> = ChangedRefMut<'w, T>;
    type ReadOnlyItem<'w> = ChangedRef<'w, T>;
    type Fetch = <&'b mut T as QueryData>::Fetch;

    fn matches(archetype: &Archetype) -> bool {
        <&mut T as QueryData>::matches(archetype)
    }

    fn collect_access(
        reads: &mut AccessVec<std::any::TypeId>,
        writes: &mut AccessVec<std::any::TypeId>,
    ) {
        <&mut T as QueryData>::collect_access(reads, writes);
        register_changed_tracked_component::<T>();
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        <&mut T as QueryData>::init_fetch(archetype)
    }

    #[inline(always)]
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        let mut_item = unsafe { <&mut T as QueryData>::fetch_mut(fetch, index) };
        let changed = unsafe { (&*mut_item.changed_state).read };
        ChangedRefMut {
            value: mut_item,
            changed,
        }
    }

    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        let read_only_item = unsafe { <&mut T as QueryData>::fetch_read_only(fetch, index) };
        let changed = unsafe { (&*fetch.1.as_ref().unwrap().as_ptr().add(index)).read };
        let tracker = ChangedTracker::<T> {
            changed,
            _marker: PhantomData,
        };

        ChangedRef {
            value: read_only_item,
            changed: tracker.changed,
        }
    }
}

/// A query filter that matches components of type `T` that were mutably modified during the previous frame.
///
/// # Architecture & Timing
///
/// Change detection in avilex is globally double-buffered and strictly time-bound. It is **not** tracking
/// individual systems or reading history. The visibility window follows a strict 3-frame lifecycle:
///
/// * **Frame 1 (Modification):** The component is mutably accessed or modified. The change is registered internally but is **not** yet visible to queries.
/// * **Frame 2 (Detection Window):** The change becomes globally visible. Any system running in this frame filtering for `Changed<T>` will detect it.
/// * **Frame 3 (Purge):** The change state is unconditionally cleared.
///
/// Regardless of whether a system ran or read the data, the detection flag will never last for more than exactly one frame.
///
/// # Structural Mutations & Spawning
///
/// Spawning a new entity with a component or inserting a component via commands acts as an initial mutation event.
/// This structural modification triggers change tracking identically to a manual component mutation:
///
/// 1. **Command Queuing:** You call `.spawn()` or `.insert()` to queue a component modification command.
/// 2. **Immediate Application:** The command is flushed and applied to the world during that same frame's execution phase.
/// 3. **Next-Frame Visibility:** The tracking states are flagged immediately following application, making `Changed<T>` evaluate to `true` across the entirety of the next frame.
pub struct Changed<T: Component>(std::marker::PhantomData<T>);

impl<T: Component> QueryFilter for Changed<T> {
    type FilterData = ComponentColumnRead<ChangedState<T>>;
    fn matches(archetype: &Archetype) -> bool {
        archetype.contains_column::<T>()
    }

    fn init_filter_data(archetype: &Archetype) -> Self::FilterData {
        archetype.column::<ChangedState<T>>()
    }

    fn matches_row(filter_data: &Self::FilterData, row_idx: usize) -> bool {
        unsafe { (*(filter_data.as_ptr().add(row_idx))).read }
    }

    fn collect_filter(withs: &mut AccessVec<TypeId>, _withouts: &mut AccessVec<TypeId>) {
        withs.push(TypeId::of::<ChangedState<T>>());
        register_changed_tracked_component::<T>();
    }

    fn collect_filter_access(reads: &mut AccessVec<TypeId>, _writes: &mut AccessVec<TypeId>) {
        reads.push(TypeId::of::<ChangedState<T>>());
    }
}

/// A mutable smart pointer to a [`Component`] of type `T` that automatically marks it as changed
/// when mutably dereferenced (`DerefMut`), if change tracking is enabled for `T`.
///
/// Immutable access via [`Deref`] does not mark the component as changed. Use
/// [`Mut::bypass_change_detection`] to obtain `&mut T` without triggering change detection, or
/// [`Mut::trigger_change_detection`] to mark it as changed manually. See [`reactivity`] for how
/// component tracking is enabled.
///
/// [`Component`]: crate::ecs::component::Component
/// [`Deref`]: std::ops::Deref
/// [`Mut::bypass_change_detection`]: Self::bypass_change_detection
/// [`Mut::trigger_change_detection`]: Self::trigger_change_detection
/// [`reactivity`]: crate::ecs::reactivity
pub struct Mut<'w, T: Component> {
    pub(crate) value: *mut T,
    pub(crate) changed_state: *mut ChangedState<T>,
    pub(crate) _marker: std::marker::PhantomData<&'w mut T>,
}

impl<'w, T: Component> Mut<'w, T> {
    /// Returns a raw mutable reference (`&mut T`) without marking the component as changed.
    #[inline(always)]
    pub fn bypass_change_detection(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }

    /// Manually marks the component as changed by setting its staged [`ChangedState`] `write` flag.
    ///
    /// [`ChangedState`]: crate::ecs::reactivity::changed::ChangedState
    #[inline(always)]
    pub fn trigger_change_detection(&mut self) {
        if !self.changed_state.is_null() {
            unsafe {
                (*self.changed_state).write = true;
            }
        }
    }
}

impl<'w, T: Component> AsRef<T> for Mut<'w, T> {
    fn as_ref(&self) -> &T {
        unsafe { &*self.value }
    }
}

unsafe impl<'w, T: Component> Send for Mut<'w, T> {}
unsafe impl<'w, T: Component> Sync for Mut<'w, T> {}

impl<'w, T: Component> std::ops::Deref for Mut<'w, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value }
    }
}

impl<'w, T: Component> std::ops::DerefMut for Mut<'w, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.trigger_change_detection();
            &mut *self.value
        }
    }
}

impl<'w, T: std::fmt::Debug + Component> std::fmt::Debug for Mut<'w, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { std::fmt::Debug::fmt(&*self.value, f) }
    }
}

impl<'w, T: std::fmt::Display + Component> std::fmt::Display for Mut<'w, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { std::fmt::Display::fmt(&*self.value, f) }
    }
}
