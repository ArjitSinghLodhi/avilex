mod concurrent_registry;

use parking_lot::RwLock;
use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    component::{Component, ComponentMut, ComponentRef},
    entity::concurrent_registry::ConcurrentRegistry,
    reactivity::changed::{ChangedState, Mut},
    world::{World, archetypes::ArchetypeId},
};

/// A lightweight, unique identifier representing an Entity ID within the ECS engine.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Entity {
    registry_index: u32,
}

impl Entity {
    pub(crate) fn new(registry_index: u32) -> Self {
        Self { registry_index }
    }

    #[inline(always)]
    pub fn registry_index(&self) -> u32 {
        self.registry_index
    }
}

impl Clone for Entity {
    fn clone(&self) -> Self {
        unsafe {
            let handle_count_ptr =
                ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(self.registry_index() as usize);
            (*handle_count_ptr).0.fetch_add(1, Ordering::Relaxed);
        }
        Entity::new(self.registry_index)
    }
}

impl Drop for Entity {
    fn drop(&mut self) {
        unsafe {
            let handle_count_ptr =
                ENTITY_HANDLE_COUNT_REGISTRY.get_ptr(self.registry_index() as usize);
            (*handle_count_ptr).0.fetch_sub(1, Ordering::Release);
        }
    }
}

pub(crate) struct EntityLocationData {
    archetype_id: ArchetypeId,
    index: u32,
}

impl EntityLocationData {
    pub(crate) fn new(archetype_id: ArchetypeId, index: u32) -> Self {
        Self {
            archetype_id,
            index,
        }
    }

    pub(crate) fn archetype_id(&self) -> ArchetypeId {
        self.archetype_id
    }

    pub(crate) fn index(&self) -> u32 {
        self.index
    }

    pub(crate) fn set_archetype_id(&mut self, archetype_id: ArchetypeId) {
        self.archetype_id = archetype_id;
    }

    pub(crate) fn set_index(&mut self, index: u32) {
        self.index = index;
    }
}

pub(crate) struct HandleCount(AtomicU32);

impl HandleCount {
    pub(crate) fn new(count: u32) -> Self {
        Self(AtomicU32::new(count))
    }

    pub(crate) fn get_count(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn set_count(&self, count: u32) {
        self.0.store(count, Ordering::Release);
    }

    pub(crate) fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

pub(crate) static ENTITY_LOCATION_REGISTRY: RwLock<Vec<EntityLocationData>> =
    RwLock::new(Vec::new());
pub(crate) static ENTITY_HANDLE_COUNT_REGISTRY: ConcurrentRegistry<HandleCount> =
    ConcurrentRegistry::new();

pub struct EntityRef<'w> {
    pub(crate) world: &'w World,
    pub(crate) entity: &'w Entity,
}

impl<'w> EntityRef<'w> {
    pub fn get<'a, T: Component>(&'a self) -> Option<ComponentRef<'a, T>> {
        let entity_location_data_ptr = unsafe {
            let ptr = ENTITY_LOCATION_REGISTRY.data_ptr();
            (*ptr).as_ptr().add(self.entity.registry_index() as usize)
        };
        let arch_id = unsafe { (*entity_location_data_ptr).archetype_id() };
        let row_index = unsafe { (*entity_location_data_ptr).index() };
        let arch = self.world.archetypes_manager.get_mut(arch_id).unwrap();
        let component_column = arch.get_column::<T>()?;
        let component = unsafe { component_column.get_unchecked(row_index as usize) as *const T };
        Some(ComponentRef {
            _guard: component_column,
            ptr: component,
        })
    }

    pub fn entity_id(&self) -> &Entity {
        self.entity
    }

    pub fn contains_component<T: Component>(&self) -> bool {
        let registry_read = ENTITY_LOCATION_REGISTRY.read();
        let entity_location_data_ptr = unsafe {
            registry_read
                .as_ptr()
                .add(self.entity.registry_index() as usize)
        };
        let arch_id = unsafe { (*entity_location_data_ptr).archetype_id() };
        let Some(arch) = self.world.archetypes_manager.get_mut(arch_id) else {
            return false;
        };
        arch.contains_column::<T>()
    }
}

impl<'w> From<EntityMut<'w>> for EntityRef<'w> {
    fn from(value: EntityMut<'w>) -> Self {
        value.entity_ref
    }
}

pub struct EntityMut<'w> {
    pub(crate) entity_ref: EntityRef<'w>,
}

impl<'w> EntityMut<'w> {
    pub fn get<'a, T: Component>(&'a self) -> Option<ComponentRef<'a, T>> {
        self.entity_ref.get::<T>()
    }
    pub fn get_mut<'a, T: Component>(&'a self) -> Option<ComponentMut<'a, T>> {
        let registry_read = ENTITY_LOCATION_REGISTRY.read();
        let entity_location_data_ptr = unsafe {
            registry_read
                .as_ptr()
                .add(self.entity_ref.entity.registry_index() as usize)
        };
        let arch_id = unsafe { (*entity_location_data_ptr).archetype_id() };
        let row_index = unsafe { (*entity_location_data_ptr).index() };
        let arch = self
            .entity_ref
            .world
            .archetypes_manager
            .get_mut(arch_id)
            .unwrap();
        let mut component_column = arch.get_column_mut::<T>()?;
        let component = unsafe { component_column.get_unchecked_mut(row_index as usize) as *mut T };
        let mut changed_state_column_opt = arch.get_column_mut::<ChangedState<T>>();
        let changed_state = if let Some(ref mut changed_state_column) = changed_state_column_opt {
            unsafe {
                changed_state_column.get_unchecked_mut(row_index as usize) as *mut ChangedState<T>
            }
        } else {
            std::ptr::null_mut()
        };
        let item = Mut {
            value: component,
            changed_state,
            _marker: PhantomData,
        };
        Some(ComponentMut {
            _guard_val: component_column,
            _guard_marker: changed_state_column_opt,
            item,
        })
    }

    pub fn entity_id(&self) -> &Entity {
        self.entity_ref.entity_id()
    }

    pub fn contains_component<T: Component>(&self) -> bool {
        self.entity_ref.contains_component::<T>()
    }
}
