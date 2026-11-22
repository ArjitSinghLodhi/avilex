use crate::ecs::component::Component;
use crate::reactivity::changed::{ChangedState, Mut};
use crate::world::archetypes::ComponentColumnWrite;
use crate::world::archetypes::{ComponentColumnRead, EntitiesRead};
use crate::{entity::Entity, query::QueryData, system::AccessVec, world::archetypes::Archetype};

use std::{any::TypeId, marker::PhantomData};

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;
    type ReadOnlyItem<'w> = &'w T;
    type Fetch = ComponentColumnRead<T>;

    fn matches(archetype: &Archetype) -> bool {
        archetype.contains_column::<T>()
    }
    fn collect_access(
        reads: &mut AccessVec<std::any::TypeId>,
        _writes: &mut AccessVec<std::any::TypeId>,
    ) {
        reads.push(TypeId::of::<T>());
    }
    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        archetype.column::<T>()
    }
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        unsafe { &*fetch.as_ptr().add(index) }
    }
    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        unsafe { &*fetch.as_ptr().add(index) }
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = Mut<'w, T>;
    type ReadOnlyItem<'w> = &'w T;
    type Fetch = (
        ComponentColumnWrite<T>,
        Option<ComponentColumnWrite<ChangedState<T>>>,
    );

    fn matches(archetype: &Archetype) -> bool {
        archetype.contains_column::<T>()
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        let data_writer = archetype.column_mut::<T>();
        let marker_writer_opt = archetype.get_column_mut::<ChangedState<T>>();

        if let Some(marker_writer) = marker_writer_opt {
            (data_writer, Some(marker_writer))
        } else {
            (data_writer, None)
        }
    }

    #[inline(always)]
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        let val_ptr = fetch.0.as_ptr() as *mut T;

        let final_marker = fetch
            .1
            .as_ref()
            .map(|marker_column| unsafe {
                (marker_column.as_ptr() as *mut ChangedState<T>).add(index)
            })
            .unwrap_or(std::ptr::null_mut());

        unsafe {
            Mut {
                value: val_ptr.add(index),
                changed_state: final_marker,
                _marker: std::marker::PhantomData,
            }
        }
    }

    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        unsafe { &*fetch.0.as_ptr().add(index) }
    }

    fn collect_access(_reads: &mut AccessVec<TypeId>, writes: &mut AccessVec<TypeId>) {
        writes.push(TypeId::of::<T>());
        writes.push(TypeId::of::<ChangedState<T>>());
    }
}

impl QueryData for Entity {
    type Item<'w> = &'w Entity;
    type ReadOnlyItem<'w> = &'w Entity;
    type Fetch = EntitiesRead;

    fn matches(_archetype: &Archetype) -> bool {
        true
    }
    fn collect_access(_reads: &mut AccessVec<TypeId>, _writes: &mut AccessVec<TypeId>) {}
    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        archetype.entities()
    }
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        unsafe { &*fetch.as_ptr().add(index) }
    }
    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        unsafe { &*fetch.as_ptr().add(index) }
    }
}

impl<T: Component> QueryData for Option<&T> {
    type Item<'w> = Option<&'w T>;
    type ReadOnlyItem<'w> = Option<&'w T>;
    type Fetch = Option<ComponentColumnRead<T>>;

    fn matches(_archetype: &Archetype) -> bool {
        true
    }

    fn collect_access(reads: &mut AccessVec<TypeId>, _writes: &mut AccessVec<TypeId>) {
        reads.push(TypeId::of::<T>());
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        if archetype.types.contains(&TypeId::of::<T>()) {
            Some(archetype.column::<T>())
        } else {
            None
        }
    }
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        if let Some(fetch) = fetch {
            let val_ptr = fetch.as_ptr() as *mut T;
            unsafe { Some(&*val_ptr.add(index)) }
        } else {
            None
        }
    }

    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        if let Some(fetch) = fetch {
            unsafe { Some(&*fetch.as_ptr().add(index)) }
        } else {
            None
        }
    }
}

impl<T: Component> QueryData for Option<&mut T> {
    type Item<'w> = Option<Mut<'w, T>>;
    type ReadOnlyItem<'w> = Option<&'w T>;
    type Fetch = Option<(
        ComponentColumnWrite<T>,
        Option<ComponentColumnWrite<ChangedState<T>>>,
    )>;

    fn matches(_archetype: &Archetype) -> bool {
        true
    }

    fn collect_access(_reads: &mut AccessVec<TypeId>, writes: &mut AccessVec<TypeId>) {
        writes.push(TypeId::of::<T>());
        writes.push(TypeId::of::<ChangedState<T>>());
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        let marker_writer_opt = archetype.get_column_mut::<ChangedState<T>>();
        let data_writer_opt = archetype.get_column_mut::<T>();
        if let Some(data_writer) = data_writer_opt {
            if let Some(marker_writer) = marker_writer_opt {
                Some((data_writer, Some(marker_writer)))
            } else {
                Some((data_writer, None))
            }
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
        unsafe {
            if let Some(fetch) = fetch {
                let data_ptr = fetch.0.as_ptr() as *mut T;

                let final_marker = fetch
                    .1
                    .as_ref()
                    .map(|marker_column| {
                        (marker_column.as_ptr() as *mut ChangedState<T>).add(index)
                    })
                    .unwrap_or(std::ptr::null_mut());

                Some(Mut {
                    value: data_ptr.add(index),
                    changed_state: final_marker,
                    _marker: std::marker::PhantomData,
                })
            } else {
                None
            }
        }
    }

    #[inline(always)]
    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
        unsafe {
            if let Some(data_head) = fetch {
                Some(&*data_head.0.as_ptr().add(index))
            } else {
                None
            }
        }
    }
}

/// A [`QueryData`] item that yields `true` if the entity possesses [`Component`] `T`, or `false`
/// otherwise, without borrowing the component data.
///
/// [`QueryData`]: crate::ecs::query::QueryData
/// [`Component`]: crate::ecs::component::Component
pub struct Has<T: Component>(PhantomData<T>);

impl<T: Component> QueryData for Has<T> {
    type Item<'w> = bool;
    type ReadOnlyItem<'w> = bool;
    type Fetch = bool;
    fn collect_access(
        _reads: &mut AccessVec<std::any::TypeId>,
        _writes: &mut AccessVec<std::any::TypeId>,
    ) {
    }

    fn matches(archetype: &Archetype) -> bool {
        archetype.contains_column::<T>()
    }

    fn init_fetch(archetype: &Archetype) -> Self::Fetch {
        archetype.contains_column::<T>()
    }

    unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, _index: usize) -> Self::Item<'a> {
        *fetch
    }

    unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, _index: usize) -> Self::ReadOnlyItem<'a> {
        *fetch
    }
}

macro_rules! impl_query_data_tuple {
    ($($name:ident -> $idx:tt),*) => {
        impl<$($name: QueryData),*> QueryData for ($($name,)*) {
            type Item<'w> = ($($name::Item<'w>,)*);
            type ReadOnlyItem<'w> = ($($name::ReadOnlyItem<'w>,)*);
            type Fetch = ($($name::Fetch,)*);

            fn matches(archetype: &Archetype) -> bool {
                $($name::matches(archetype))&&*
            }
            fn init_fetch(archetype: &Archetype) -> Self::Fetch {
                (
                    $($name::init_fetch(archetype),)*
                )
            }
            unsafe fn fetch_mut<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::Item<'a> {
                unsafe {($($name::fetch_mut(&fetch.$idx, index),)*)}
            }

            unsafe fn fetch_read_only<'a>(fetch: &'a Self::Fetch, index: usize) -> Self::ReadOnlyItem<'a> {
                unsafe {
                    (
                        $($name::fetch_read_only(&fetch.$idx, index),)*
                    )
                }
            }

            fn collect_access(reads: &mut AccessVec<TypeId>, writes: &mut AccessVec<TypeId>) {
                $( $name::collect_access(reads, writes); )*
            }
        }
    };
}

impl_query_data_tuple!(A -> 0);
impl_query_data_tuple!(A -> 0, B -> 1);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6, H -> 7);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6, H -> 7, I -> 8);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6, H -> 7, I -> 8, J -> 9);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6, H -> 7, I -> 8, J -> 9, K -> 10);
impl_query_data_tuple!(A -> 0, B -> 1, C -> 2, D -> 3, E -> 4, F -> 5, G -> 6, H -> 7, I -> 8, J -> 9, K -> 10, L -> 11);
