use indexmap::IndexMap;
use indexmap::IndexSet;
use parking_lot::MutexGuard;
use parking_lot::{RwLock, RwLockWriteGuard};
use rustc_hash::FxBuildHasher;
use std::any::TypeId;
use std::marker::PhantomData;

use crate::entity::ENTITY_HANDLE_COUNT_REGISTRY;
use crate::entity::ENTITY_LOCATION_REGISTRY;
use crate::entity::EntityLocationData;
use crate::entity::HandleCount;
use crate::reactivity::TrackedComponentMeta;
use crate::reactivity::{TRACKED_COMPONENTS, removed::REMOVAL_TRACKED_COMPS};
use crate::world::archetypes::ArchetypeLock;
use crate::{
    commands::{Command, bundle::ComponentBundle},
    component::hooks::ExclusiveWorld,
    entity::Entity,
    world::{
        World,
        archetypes::{AnyColumn, Archetype, ArchetypeId, ComponentColumn},
    },
};

pub(crate) struct SpawnCommandCore<T: ComponentBundle> {
    pub(crate) components: T,
}

impl<T: ComponentBundle> Command for SpawnCommandCore<T> {
    type Out = Entity;
    fn apply(self, world: &mut World) -> Self::Out {
        let arch_id = world.archetypes_manager.get_or_create_from_generic::<T>();
        let next_idx = match world.archetypes_manager.get_mut(arch_id) {
            Some(arch) => arch.entities.read().len() as u32,
            None => 0,
        };
        let assigned_registry_idx = alloc_registry_cell(arch_id, next_idx, world);
        let mut arch = world
            .archetypes_manager
            .get_mut(arch_id)
            .expect("Archetype generation failed");
        let entity = Entity::new(assigned_registry_idx);
        let row_idx = {
            let mut entities_lock = arch.entities.write();
            let row_idx = entities_lock.len();
            entities_lock.push(entity.clone());
            row_idx
        };
        {
            let columns = &mut arch.columns;
            initialize_spawn_states(&TRACKED_COMPONENTS.lock(), columns);
        }
        unsafe { self.components.insert_to_archetype(&mut arch, row_idx) };

        drop(arch);

        let type_ids = T::get_type_ids();

        let mut must_register = false;
        for &id in type_ids {
            if !world.component_hooks_registry.is_auto_registered(id) {
                must_register = true;
                break;
            }
        }

        if must_register {
            T::register_bundle_hooks(world);
        }

        for &id in type_ids {
            let (on_add, on_insert) = {
                if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    (hooks.on_add, hooks.on_insert)
                } else {
                    continue;
                }
            };

            let mut exclusive_world = ExclusiveWorld { world };
            for on_add_hook in on_add.iter() {
                on_add_hook(&mut exclusive_world, &entity);
            }
            for on_insert_hook in on_insert.iter() {
                on_insert_hook(&mut exclusive_world, &entity);
            }
        }
        entity
    }
}

pub(crate) struct SpawnCommand<T: ComponentBundle> {
    pub(crate) spawn_command_core: SpawnCommandCore<T>,
}

impl<T: ComponentBundle> Command for SpawnCommand<T> {
    type Out = ();
    fn apply(self, world: &mut World) -> Self::Out {
        self.spawn_command_core.apply(world);
    }
}

pub(crate) struct BatchSpawnCommandCore<T, I>
where
    T: ComponentBundle,
    I: Iterator<Item = T> + Send + 'static,
{
    pub(crate) components_iter: I,
}

impl<T: ComponentBundle, I: Iterator<Item = T> + Send + 'static> Command
    for BatchSpawnCommandCore<T, I>
{
    type Out = Vec<Entity>;
    fn apply(self, world: &mut World) -> Self::Out {
        let world_ptr = world as *mut World;
        let arch_id = unsafe {
            (*world_ptr)
                .archetypes_manager
                .get_or_create_from_generic::<T>()
        };

        let mut arch = unsafe {
            (*world_ptr)
                .archetypes_manager
                .get_mut(arch_id)
                .expect("Archetype generation failed")
        };

        let (lower_bound, _) = self.components_iter.size_hint();
        if lower_bound > 0 {
            arch.entities.write().reserve(lower_bound);
        }

        let mut spawned_entities = Vec::new();
        if lower_bound > 0 {
            spawned_entities.reserve(lower_bound);
        }
        let tracked_components_guard = TRACKED_COMPONENTS.lock();
        for components in self.components_iter {
            let entity = {
                let mut arch_entities = arch.entities.write();
                let next_idx = arch_entities.len() as u32;
                let assigned_registry_idx =
                    unsafe { alloc_registry_cell(arch_id, next_idx, &mut *world_ptr) };
                let entity = Entity::new(assigned_registry_idx);
                arch_entities.push(entity.clone());
                entity
            };

            let row_idx = arch.entities.read().len() - 1;
            initialize_spawn_states(&tracked_components_guard, &mut arch.columns);
            unsafe { components.insert_to_archetype(&mut arch, row_idx) };
            spawned_entities.push(entity);
        }

        drop(arch);

        let type_ids = T::get_type_ids();

        let mut must_register = false;
        for &id in type_ids {
            if !world.component_hooks_registry.is_auto_registered(id) {
                must_register = true;
                break;
            }
        }

        if must_register {
            T::register_bundle_hooks(world);
        }

        for entity in &spawned_entities {
            for &id in type_ids {
                let (on_add, on_insert) = {
                    if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                        (hooks.on_add, hooks.on_insert)
                    } else {
                        continue;
                    }
                };

                let mut exclusive_world = ExclusiveWorld { world };
                for on_add_hook in on_add.iter() {
                    on_add_hook(&mut exclusive_world, entity);
                }
                for on_insert_hook in on_insert.iter() {
                    on_insert_hook(&mut exclusive_world, entity);
                }
            }
        }
        spawned_entities
    }
}

pub(crate) struct BatchSpawnCommand<T: ComponentBundle, I>
where
    T: ComponentBundle,
    I: Iterator<Item = T> + Send + 'static,
{
    pub(crate) batch_spawn_command_core: BatchSpawnCommandCore<T, I>,
}

impl<T: ComponentBundle, I> Command for BatchSpawnCommand<T, I>
where
    T: ComponentBundle,
    I: Iterator<Item = T> + Send + 'static,
{
    type Out = ();
    fn apply(self, world: &mut World) -> Self::Out {
        self.batch_spawn_command_core.apply(world);
    }
}

const ENTITY_HANDLE_VIOLATION_ERR: &str = "AvilexEntityHandleViolation";

pub(crate) struct DespawnCommand {
    pub(crate) entity: Entity,
}

impl Command for DespawnCommand {
    type Out = ();
    fn apply(self, world: &mut World) -> Self::Out {
        let target_registry_idx = self.entity.registry_index() as usize;
        unsafe {
            (*ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(target_registry_idx)).decrement();
        }

        let initial_arch_id = unsafe {
            let registry_ptr = ENTITY_LOCATION_REGISTRY.data_ptr();
            let data_ptr = (*registry_ptr).as_ptr().add(target_registry_idx);
            (*data_ptr).archetype_id()
        };

        let arch = world
            .archetypes_manager
            .get_mut(initial_arch_id)
            .expect("Target archetype missing");

        let type_ids = arch.types.clone();
        drop(arch);

        for &id in type_ids.iter() {
            let on_remove = {
                if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    hooks.on_remove
                } else {
                    continue;
                }
            };

            let mut exclusive_world = ExclusiveWorld { world };
            for on_remove_hook in on_remove.iter() {
                on_remove_hook(&mut exclusive_world, &self.entity);
            }
        }

        let handle_count =
            (unsafe { &*ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(target_registry_idx) }).get_count();
        if handle_count > 0 {
            let arch = &&world.archetypes_manager.get_mut(initial_arch_id).unwrap();
            let types_names = &arch.type_names;
            panic!(
                "{}\n\
                Cloned `Entity` handles were not dropped before the handle count check!\n\
                Entity Archetype Component Names:\n\
                {:#?}\n\
                Refer to the `on_remove` hook inside the documentation for the `Component` trait to understand how to coordinate handle cleanup.",
                ENTITY_HANDLE_VIOLATION_ERR, types_names
            );
        }

        let mut entity_location_guard = ENTITY_LOCATION_REGISTRY.write();
        let (current_arch_id, current_target_idx) = unsafe {
            let data_ptr = entity_location_guard.as_ptr().add(target_registry_idx);
            ((*data_ptr).archetype_id(), (*data_ptr).index())
        };

        let mut arch = world
            .archetypes_manager
            .get_mut(current_arch_id)
            .expect("Target archetype missing");

        let last_idx = (arch.entities.read().len() - 1) as u32;

        if current_target_idx != last_idx {
            let swapped_entity_registry_idx =
                arch.entities.read()[last_idx as usize].registry_index() as usize;

            unsafe {
                let swapped_entity_location_data_ptr = entity_location_guard
                    .as_mut_ptr()
                    .add(swapped_entity_registry_idx);
                (*swapped_entity_location_data_ptr).set_index(current_target_idx);
            }
        }
        unsafe {
            let cols = &mut arch.columns;
            for col in cols.values_mut() {
                col.data
                    .write()
                    .swap_remove_erased(current_target_idx as usize);
            }
        }

        arch.entities
            .write()
            .swap_remove(current_target_idx as usize);
        world.free_indices_list.push(target_registry_idx as u32);
    }
}

pub(crate) struct InsertComponentsCommand<T: ComponentBundle> {
    pub(crate) entity: Entity,
    pub(crate) components: T,
}

impl<T: ComponentBundle> Command for InsertComponentsCommand<T> {
    type Out = ();
    fn apply(self, world: &mut World) -> Self::Out {
        let incoming_ids = T::get_type_ids();
        let target_registry_idx = self.entity.registry_index() as usize;

        let mut must_register = false;
        for &id in incoming_ids {
            if !world.component_hooks_registry.is_auto_registered(id) {
                must_register = true;
                break;
            }
        }

        if must_register {
            T::register_bundle_hooks(world);
        }

        let (mut old_arch_id, mut old_idx) = get_registry_location(target_registry_idx);

        let mut new_arch_id = if let Some(id) = world
            .archetypes_manager
            .find_target_id_for_addition(old_arch_id, incoming_ids)
        {
            id
        } else {
            create_addition_archetype::<T>(world, old_arch_id, incoming_ids)
        };

        if old_arch_id == new_arch_id {
            for &id in incoming_ids {
                let on_discard = if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    hooks.on_discard
                } else {
                    continue;
                };

                let mut exclusive_world = ExclusiveWorld { world };
                for on_discard_hook in on_discard.iter() {
                    on_discard_hook(&mut exclusive_world, &self.entity);
                }
            }

            let (fresh_arch_id, fresh_idx) = get_registry_location(target_registry_idx);

            let mut arch = world.archetypes_manager.get_mut(fresh_arch_id).expect(
                "avilex Engine Fatal: Entity registry pointed to an untracked Archetype ID",
            );

            unsafe {
                self.components
                    .insert_to_archetype(&mut arch, fresh_idx as usize);
            }

            drop(arch);

            for &id in incoming_ids {
                let on_insert = if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    hooks.on_insert
                } else {
                    continue;
                };

                let mut exclusive_world = ExclusiveWorld { world };
                for on_insert_hook in on_insert.iter() {
                    on_insert_hook(&mut exclusive_world, &self.entity);
                }
            }
            return;
        }

        let mut old_has_component = Vec::with_capacity(incoming_ids.len());
        {
            let old_arch = world
                .archetypes_manager
                .get_mut(old_arch_id)
                .expect("Old archetype missing");
            for &id in incoming_ids {
                old_has_component.push(old_arch.types.contains(&id));
            }
        }

        for (i, &id) in incoming_ids.iter().enumerate() {
            if old_has_component[i] {
                let on_discard = if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    hooks.on_discard
                } else {
                    continue;
                };

                let mut exclusive_world = ExclusiveWorld { world };
                for on_discard_hook in on_discard.iter() {
                    on_discard_hook(&mut exclusive_world, &self.entity);
                }
            }
        }

        let (fresh_arch_id, fresh_idx) = get_registry_location(target_registry_idx);

        if fresh_arch_id != old_arch_id {
            old_arch_id = fresh_arch_id;
            old_idx = fresh_idx;
            new_arch_id = if let Some(id) = world
                .archetypes_manager
                .find_target_id_for_addition(old_arch_id, incoming_ids)
            {
                id
            } else {
                create_addition_archetype::<T>(world, old_arch_id, incoming_ids)
            };
        }

        unsafe {
            let (mut old_arch, mut new_arch) =
                get_double_archetypes(world, old_arch_id, new_arch_id);
            let new_dense_idx = {
                let Archetype {
                    columns: old_cols, ..
                } = &mut *old_arch;
                let Archetype {
                    entities: new_arch_entities,
                    columns: new_cols,
                    ..
                } = &mut *new_arch;
                let new_arch_entities = new_arch_entities.read();
                move_matching_columns(old_cols, new_cols, old_idx as usize);
                new_arch_entities.len() as u32
            };

            let fresh_new_cols = &mut new_arch.columns;
            migrate_addition_states(&old_arch.types, incoming_ids, fresh_new_cols);

            self.components
                .insert_to_archetype(&mut new_arch, new_dense_idx as usize);

            let mut old_arch_entities = old_arch.entities.write();
            swap_remove_entity_registry_update(&mut old_arch_entities, old_idx);
            let entity_handle = old_arch_entities.swap_remove(old_idx as usize);
            new_arch.entities.write().push(entity_handle);
            update_registry_cell(target_registry_idx, new_arch_id, new_dense_idx);
        }

        for (i, &id) in incoming_ids.iter().enumerate() {
            let (on_add, on_insert) =
                if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                    (hooks.on_add, hooks.on_insert)
                } else {
                    continue;
                };

            let mut exclusive_world = ExclusiveWorld { world };
            if !old_has_component[i] {
                for on_add_hook in on_add.iter() {
                    on_add_hook(&mut exclusive_world, &self.entity);
                }
            }
            for on_insert_hook in on_insert.iter() {
                on_insert_hook(&mut exclusive_world, &self.entity);
            }
        }
    }
}

pub(crate) struct RemoveComponentsCommand<T: ComponentBundle> {
    pub(crate) entity: Entity,
    pub(crate) _marker: PhantomData<T>,
}

impl<T: ComponentBundle> Command for RemoveComponentsCommand<T> {
    type Out = ();
    fn apply(self, world: &mut World) -> Self::Out {
        let target_registry_idx = self.entity.registry_index() as usize;
        let removed_ids = T::get_type_ids();

        let mut must_register = false;
        for &id in removed_ids {
            if !world.component_hooks_registry.is_auto_registered(id) {
                must_register = true;
                break;
            }
        }

        if must_register {
            T::register_bundle_hooks(world);
        }

        for &id in removed_ids {
            let on_remove = if let Some(hooks) = world.component_hooks_registry.get_hooks(id) {
                hooks.on_remove
            } else {
                continue;
            };

            let mut exclusive_world = ExclusiveWorld { world };
            for on_remove_hook in on_remove.iter() {
                on_remove_hook(&mut exclusive_world, &self.entity);
            }
        }

        let (old_arch_id, old_idx) = get_registry_location(target_registry_idx);

        let new_arch_id = if let Some(id) = world
            .archetypes_manager
            .find_target_id_for_subtraction(old_arch_id, removed_ids)
        {
            id
        } else {
            create_subtraction_archetype::<T>(world, old_arch_id, removed_ids)
        };

        if old_arch_id == new_arch_id {
            return;
        }

        unsafe {
            let world_ptr = world as *mut World;
            let (mut old_arch, mut new_arch) =
                get_double_archetypes(world, old_arch_id, new_arch_id);
            let Archetype {
                entities: new_arch_entities,
                columns: new_cols,
                types: new_types,
                ..
            } = &mut *new_arch;
            let mut new_arch_entities = new_arch_entities.write();
            let new_dense_idx = new_arch_entities.len() as u32;
            let Archetype {
                entities: old_arch_entities,
                columns: old_cols,
                types: old_types,
                ..
            } = &mut *old_arch;
            move_matching_columns(old_cols, new_cols, old_idx as usize);
            erase_subtracted_columns(removed_ids, old_cols, old_idx as usize);

            let mut old_arch_entities = old_arch_entities.write();

            erase_subtracted_states(old_types, new_types, old_cols, old_idx as usize);
            let entity_handle = &old_arch_entities[old_idx as usize];
            push_removed_components_entity_reactivity(world_ptr, removed_ids, entity_handle);

            swap_remove_entity_registry_update(&mut old_arch_entities, old_idx);

            let entity_handle = old_arch_entities.swap_remove(old_idx as usize);
            new_arch_entities.push(entity_handle);

            update_registry_cell(target_registry_idx, new_arch_id, new_dense_idx);
        }
    }
}

#[inline(always)]
fn get_registry_location(registry_idx: usize) -> (ArchetypeId, u32) {
    unsafe {
        let entity_location_data_ptr = (*ENTITY_LOCATION_REGISTRY.data_ptr())
            .as_ptr()
            .add(registry_idx);
        let arch_id = (*entity_location_data_ptr).archetype_id();
        (arch_id, (*entity_location_data_ptr).index())
    }
}

#[inline(always)]
fn update_registry_cell(registry_idx: usize, archetype_id: ArchetypeId, dense_idx: u32) {
    unsafe {
        let mut lock = ENTITY_LOCATION_REGISTRY.write();
        let entity_location_data_ptr = lock.as_mut_ptr().add(registry_idx);
        (*entity_location_data_ptr).set_archetype_id(archetype_id);
        (*entity_location_data_ptr).set_index(dense_idx);
    }
}

#[inline(always)]
fn alloc_registry_cell(archetype_id: ArchetypeId, dense_idx: u32, world: &mut World) -> u32 {
    let mut lock = ENTITY_LOCATION_REGISTRY.write();
    if let Some(recycled_idx) = world.free_indices_list.pop() {
        unsafe {
            let data_ptr = lock.as_mut_ptr().add(recycled_idx as usize);
            (*data_ptr).set_archetype_id(archetype_id);
            (*data_ptr).set_index(dense_idx);
            let count_ptr = ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(recycled_idx as usize);
            (*count_ptr).set_count(0);
        }
        recycled_idx
    } else {
        let len = lock.len() as u32;
        lock.push(EntityLocationData::new(archetype_id, dense_idx));
        ENTITY_HANDLE_COUNT_REGISTRY.push(HandleCount::new(0));
        len
    }
}

#[inline(always)]
unsafe fn get_double_archetypes<'a>(
    world: &'a mut World,
    old_id: ArchetypeId,
    new_id: ArchetypeId,
) -> (ArchetypeLock<'a>, ArchetypeLock<'a>) {
    let manager = &mut world.archetypes_manager;
    let old_arch = manager.get_mut(old_id).expect("Old archetype missing");
    let new_arch = manager.get_mut(new_id).expect("New archetype missing");
    (old_arch, new_arch)
}

fn create_addition_archetype<T: ComponentBundle>(
    world: &mut World,
    old_arch_id: ArchetypeId,
    incoming_ids: &[TypeId],
) -> ArchetypeId {
    let mut new_types_names;
    let mut new_types;

    let mut cloned_base_cols = IndexMap::with_hasher(FxBuildHasher);
    {
        let old_arch = world
            .archetypes_manager
            .get_mut(old_arch_id)
            .expect("avilex Engine Fatal: Old Archetype ID not found");
        new_types = old_arch.types.clone();
        for id in incoming_ids {
            new_types.insert(*id);
        }

        world
            .archetypes_manager
            .sync_tracking_states(&mut new_types);

        new_types_names = old_arch.type_names.clone();
        for id in T::get_type_names().as_ref() {
            new_types_names.insert(id);
        }

        for (type_id, old_col) in old_arch.columns.iter() {
            cloned_base_cols.insert(
                *type_id,
                ComponentColumn {
                    data: RwLock::new(old_col.data.write().clone_empty()),
                },
            );
        }
    }

    world.archetypes_manager.get_or_create_from_set(
        new_types,
        new_types_names,
        |_types_set, cols| {
            T::create_empty_columns(cols);

            for (type_id, col) in cloned_base_cols {
                cols.entry(type_id).or_insert(col);
            }

            initialize_missing_archetype_states(_types_set, cols);
        },
    )
}

fn create_subtraction_archetype<T: ComponentBundle>(
    world: &mut World,
    old_arch_id: ArchetypeId,
    removed_ids: &[TypeId],
) -> ArchetypeId {
    let mut new_types;
    let mut new_types_names;
    let mut cloned_base_cols = IndexMap::with_hasher(FxBuildHasher);
    {
        let old_arch = world
            .archetypes_manager
            .get_mut(old_arch_id)
            .expect("avilex Engine Fatal: Old Archetype ID not found");
        new_types = old_arch.types.clone();
        for id in removed_ids {
            new_types.swap_remove(id);
        }

        world
            .archetypes_manager
            .sync_tracking_states(&mut new_types);

        new_types_names = old_arch.type_names.clone();
        for id_name in T::get_type_names().as_ref() {
            new_types_names.shift_remove(id_name);
        }

        for (type_id, old_col) in old_arch.columns.iter() {
            if new_types.contains(type_id) {
                cloned_base_cols.insert(
                    *type_id,
                    ComponentColumn {
                        data: RwLock::new(old_col.data.write().clone_empty()),
                    },
                );
            }
        }
    }

    world.archetypes_manager.get_or_create_from_set(
        new_types,
        new_types_names,
        |_types_set, cols| {
            for (type_id, col) in cloned_base_cols {
                cols.insert(type_id, col);
            }

            initialize_missing_archetype_states(_types_set, cols);
        },
    )
}

#[inline(always)]
unsafe fn move_matching_columns(
    src: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
    dst: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
    row_idx: usize,
) {
    for (type_id, old_col) in src.iter_mut() {
        if let Some(new_col) = dst.get_mut(type_id) {
            let mut data_guard = new_col.data.write();
            let dst_ptr = &mut **data_guard as *mut dyn AnyColumn;
            unsafe { old_col.data.write().move_row_erased(row_idx, dst_ptr) };
        }
    }
}

#[inline(always)]
unsafe fn erase_subtracted_columns(
    ids: &[TypeId],
    columns: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
    row_idx: usize,
) {
    for id in ids {
        if let Some(col) = columns.get_mut(id) {
            unsafe { col.data.write().swap_remove_erased(row_idx) };
        }
    }
}

#[inline(always)]
unsafe fn swap_remove_entity_registry_update(
    arch_entities: &mut RwLockWriteGuard<Vec<Entity>>,
    removed_row_idx: u32,
) {
    let mut lock = ENTITY_LOCATION_REGISTRY.write();
    let last_idx = (arch_entities.len() - 1) as u32;
    if removed_row_idx != last_idx {
        let swapped_entity_registry_idx =
            arch_entities[last_idx as usize].registry_index() as usize;
        let swapped_cell = unsafe { lock.as_mut_ptr().add(swapped_entity_registry_idx) };
        unsafe {
            (*swapped_cell).set_index(removed_row_idx);
        }
    }
}

#[inline(always)]
fn initialize_spawn_states(
    tracked_components_guard: &MutexGuard<
        '_,
        IndexMap<TypeId, Vec<TrackedComponentMeta>, FxBuildHasher>,
    >,
    columns: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
) {
    for meta_list in tracked_components_guard.values() {
        for meta in meta_list {
            if let Some(marker_column) = columns.get_mut(&meta.marker_id) {
                unsafe { (meta.push_default_marker)(marker_column) };
            }
        }
    }
}

#[inline(always)]
fn initialize_missing_archetype_states(
    types: &IndexSet<TypeId, FxBuildHasher>,
    columns: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
) {
    let tracked = TRACKED_COMPONENTS.lock();
    for meta_list in tracked.values() {
        for meta in meta_list {
            if types.contains(&meta.marker_id) && !columns.contains_key(&meta.marker_id) {
                columns.insert(meta.marker_id, (meta.create_marker_column)());
            }
        }
    }
}

#[inline(always)]
unsafe fn migrate_addition_states(
    old_types: &IndexSet<TypeId, FxBuildHasher>,
    incoming_ids: &[TypeId],
    new_cols: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
) {
    let tracked = TRACKED_COMPONENTS.lock();

    for &comp_id in incoming_ids {
        if let Some(meta_list) = tracked.get(&comp_id) {
            for meta in meta_list {
                if new_cols.contains_key(&meta.marker_id) {
                    if old_types.contains(&meta.marker_id) {
                        continue;
                    } else if let Some(marker_column) = new_cols.get_mut(&meta.marker_id) {
                        unsafe { (meta.push_default_marker)(marker_column) };
                    }
                }
            }
        }
    }
}

#[inline(always)]
unsafe fn erase_subtracted_states(
    old_types: &IndexSet<TypeId, FxBuildHasher>,
    new_types: &IndexSet<TypeId, FxBuildHasher>,
    old_cols: &mut IndexMap<TypeId, ComponentColumn, FxBuildHasher>,
    row_idx: usize,
) {
    let tracked = TRACKED_COMPONENTS.lock();
    for meta_list in tracked.values() {
        for meta in meta_list {
            if old_types.contains(&meta.marker_id)
                && !new_types.contains(&meta.marker_id)
                && let Some(marker_col) = old_cols.get_mut(&meta.marker_id)
            {
                unsafe { marker_col.data.write().swap_remove_erased(row_idx) };
            }
        }
    }
}

pub(crate) unsafe fn push_removed_components_entity_reactivity(
    world_ptr: *mut World,
    removed_ids: &[std::any::TypeId],
    entity: &Entity,
) {
    if removed_ids.is_empty() {
        return;
    }
    let tracked = REMOVAL_TRACKED_COMPS.lock();
    if tracked.is_empty() {
        return;
    }
    let world = unsafe { &mut *world_ptr };

    for type_id in removed_ids {
        if let Some(meta) = tracked.get(type_id) {
            (meta.push_to_write_queue)(world, entity.clone());
        }
    }
}
