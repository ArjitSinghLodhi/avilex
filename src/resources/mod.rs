use crate::{
    system::{SystemAccessMeta, SystemParam, system_error::SystemParamError},
    world::World,
};
use dashmap::{DashMap, mapref::one::Ref};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use rustc_hash::FxBuildHasher;
use std::any::{Any, TypeId, type_name};

mod parallel_resources;

pub use parallel_resources::ParallelResourceAccessor;

pub(crate) struct ConcurrentResourceRegistry {
    pub(crate) resources:
        DashMap<TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>, FxBuildHasher>,
}

unsafe impl Send for ConcurrentResourceRegistry {}
unsafe impl Sync for ConcurrentResourceRegistry {}

impl ConcurrentResourceRegistry {
    pub(crate) fn new() -> Self {
        Self {
            resources: DashMap::with_hasher(FxBuildHasher),
        }
    }

    pub(crate) fn contains_resource<T: Resource + Send + Sync>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    pub(crate) fn insert_resource<T: Resource + Send + Sync>(&self, resource: T) -> Option<T> {
        let type_id = TypeId::of::<T>();
        let boxed_res = Box::new(resource);
        if let Some(res_lock) = self.resources.insert(type_id, RwLock::new(boxed_res)) {
            let boxed_any = res_lock.into_inner();
            let res = boxed_any.downcast::<T>().unwrap();
            Some(*res)
        } else {
            None
        }
    }

    pub(crate) fn remove_resource<T: Resource + Send + Sync>(&self) -> Option<T> {
        let type_id = TypeId::of::<T>();
        if let Some((_, res_lock)) = self.resources.remove(&type_id) {
            let boxed_any = res_lock.into_inner();
            let res = boxed_any.downcast::<T>().unwrap();
            Some(*res)
        } else {
            None
        }
    }

    pub(crate) fn resource<'w, T: Resource + Send + Sync>(&self) -> Res<'w, T> {
        self.get_resource().unwrap_or_else(|| {
            panic!(
                "ResourceMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    pub(crate) fn resource_mut<'w, T: Resource + Send + Sync>(&self) -> ResMut<'w, T> {
        self.get_resource_mut().unwrap_or_else(|| {
            panic!(
                "ResourceMissingError\n\
                Name: {}",
                type_name::<T>()
            );
        })
    }

    pub(crate) fn get_resource<'w, T: Resource + Send + Sync>(&self) -> Option<Res<'w, T>> {
        let type_id = TypeId::of::<T>();
        let entry = self.resources.get(&type_id)?;
        unsafe {
            let entry = std::mem::transmute::<
                Ref<'_, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
                Ref<'_, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
            >(entry);
            let raw_guard = entry.value().read();
            let guard = std::mem::transmute::<
                RwLockReadGuard<'_, Box<dyn Any + Send + Sync + 'static>>,
                RwLockReadGuard<'_, Box<dyn Any + Send + Sync + 'static>>,
            >(raw_guard);
            let res_ptr = guard.downcast_ref::<T>()? as *const T;
            Some(Res {
                _guard: guard,
                _ref_guard: entry,
                res_ptr,
            })
        }
    }

    pub(crate) fn get_resource_mut<'w, T: Resource + Send + Sync>(&self) -> Option<ResMut<'w, T>> {
        let type_id = TypeId::of::<T>();
        let entry = self.resources.get(&type_id)?;

        unsafe {
            let entry = std::mem::transmute::<
                Ref<'_, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
                Ref<'_, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
            >(entry);
            let raw_guard = entry.value().write();
            let mut guard = std::mem::transmute::<
                RwLockWriteGuard<'_, Box<dyn Any + Send + Sync + 'static>>,
                RwLockWriteGuard<'_, Box<dyn Any + Send + Sync + 'static>>,
            >(raw_guard);
            let res_ptr = guard.downcast_mut::<T>()? as *mut T;
            Some(ResMut {
                _guard: guard,
                _ref_guard: entry,
                res_ptr,
            })
        }
    }
}

/// A marker trait for global singleton data stored in a [`World`].
///
/// Derive this using `#[derive(Resource)]`.
///
/// [`World`]: crate::ecs::world::World
pub trait Resource: 'static {}

/// A shared read-only RAII guard to a thread-safe [`Resource`] of type `T`.
///
/// Can be used as a [`SystemParam`] or retrieved directly via [`World::resource`] and
/// [`World::get_resource`]. Wrap in `Option<Res<T>>` when the resource may not exist.
///
/// # Panics
/// Panics when fetched as a [`SystemParam`] if no resource of type `T` exists in the [`World`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`World`]: crate::ecs::world::World
/// [`World::resource`]: crate::ecs::world::World::resource
/// [`World::get_resource`]: crate::ecs::world::World::get_resource
pub struct Res<'w, T: Resource + Send + Sync> {
    _guard: RwLockReadGuard<'w, Box<dyn Any + Send + Sync + 'static>>,
    _ref_guard: Ref<'w, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
    res_ptr: *const T,
}

impl<'w, T: Resource + Send + Sync> std::ops::Deref for Res<'w, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.res_ptr) }
    }
}

unsafe impl<'w, T: Resource + Send + Sync> Send for Res<'w, T> {}
unsafe impl<'w, T: Resource + Send + Sync> Sync for Res<'w, T> {}

/// An exclusive mutable RAII guard to a thread-safe [`Resource`] of type `T`.
///
/// Can be used as a [`SystemParam`] or retrieved directly via [`World::resource_mut`] and
/// [`World::get_resource_mut`]. Wrap in `Option<ResMut<T>>` when the resource may not exist.
///
/// # Panics
/// Panics when fetched as a [`SystemParam`] if no resource of type `T` exists in the [`World`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`World`]: crate::ecs::world::World
/// [`World::resource_mut`]: crate::ecs::world::World::resource_mut
/// [`World::get_resource_mut`]: crate::ecs::world::World::get_resource_mut
pub struct ResMut<'w, T: Resource + Send + Sync> {
    _guard: RwLockWriteGuard<'w, Box<dyn Any + Send + Sync + 'static>>,
    _ref_guard: Ref<'w, TypeId, RwLock<Box<dyn Any + Send + Sync + 'static>>>,
    res_ptr: *mut T,
}

impl<'w, T: Resource + Send + Sync> std::ops::Deref for ResMut<'w, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.res_ptr) }
    }
}

impl<'w, T: Resource + Send + Sync> std::ops::DerefMut for ResMut<'w, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut (*self.res_ptr) }
    }
}

unsafe impl<'w, T: Resource + Send + Sync> Send for ResMut<'w, T> {}
unsafe impl<'w, T: Resource + Send + Sync> Sync for ResMut<'w, T> {}

/// An exclusive mutable reference to a main-thread (`NonSend`) [`Resource`] of type `T`.
///
/// Systems requesting `NonSendResMut<T>` are automatically scheduled on the main thread. Wrap in
/// `Option<NonSendResMut<T>>` when the resource may not exist.
///
/// # Panics
/// Panics when fetched as a [`SystemParam`] if no resource of type `T` exists in the [`World`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`World`]: crate::ecs::world::World
pub struct NonSendResMut<'w, T: Resource> {
    pub(crate) res: &'w mut T,
}

impl<'w, T: Resource> std::ops::Deref for NonSendResMut<'w, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.res
    }
}

impl<'w, T: Resource> std::ops::DerefMut for NonSendResMut<'w, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.res
    }
}

/// A shared read-only reference to a main-thread (`NonSend`) [`Resource`] of type `T`.
///
/// Systems requesting `NonSendRes<T>` are automatically scheduled on the main thread. Wrap in
/// `Option<NonSendRes<T>>` when the resource may not exist.
///
/// # Panics
/// Panics when fetched as a [`SystemParam`] if no resource of type `T` exists in the [`World`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`World`]: crate::ecs::world::World
pub struct NonSendRes<'w, T: Resource> {
    pub(crate) res: &'w T,
}

impl<'w, T: Resource> std::ops::Deref for NonSendRes<'w, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.res
    }
}

impl<'w, T: Resource + Send + Sync> SystemParam for Res<'w, T> {
    type Item<'world> = Res<'world, T>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        let res_id = TypeId::of::<T>();
        if system_meta.has_resource_write(&res_id) {
            panic!(
                "ResourceBorrowConflictError\n\
                System: {}\n\
                Resource: {}",
                system_meta.system_name(),
                std::any::type_name::<T>()
            );
        }
        system_meta.add_resource_read(res_id);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.resource::<T>())
    }
}

impl<'w, T: Resource + Send + Sync> SystemParam for ResMut<'w, T> {
    type Item<'world> = ResMut<'world, T>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        let res_id = TypeId::of::<T>();
        if system_meta.has_resource_write(&res_id) || system_meta.has_resource_read(&res_id) {
            panic!(
                "ResourceBorrowConflictError\n\
                System: {}\n\
                Resource: {}",
                system_meta.system_name(),
                std::any::type_name::<T>()
            );
        }
        system_meta.add_resource_write(res_id);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.resource_mut::<T>())
    }
}

impl<'w, T: Resource> SystemParam for NonSendRes<'w, T> {
    type Item<'world> = NonSendRes<'world, T>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        let res_id = TypeId::of::<T>();
        if system_meta.has_resource_write(&res_id) {
            panic!(
                "NonSendResResourceBorrowConflictError\n\
                System: {}\n\
                Resource: {}",
                system_meta.system_name(),
                std::any::type_name::<T>()
            );
        }
        system_meta.add_resource_read(res_id);
        system_meta.set_non_send();
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.non_send_resource::<T>())
    }
}

impl<'w, T: Resource> SystemParam for NonSendResMut<'w, T> {
    type Item<'world> = NonSendResMut<'world, T>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        let res_id = TypeId::of::<T>();
        if system_meta.has_resource_write(&res_id) || system_meta.has_resource_read(&res_id) {
            panic!(
                "NonSendResResourceBorrowConflictError\n\
                System: {}\n\
                Resource: {}",
                system_meta.system_name(),
                std::any::type_name::<T>()
            );
        }
        system_meta.add_resource_write(res_id);
        system_meta.set_non_send();
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(unsafe { world.non_send_resource_mut_unsafe::<T>() })
    }
}

impl<'w, T: Resource + Send + Sync> SystemParam for Option<Res<'w, T>> {
    type Item<'world> = Option<Res<'world, T>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Res::<T>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.get_resource::<T>())
    }
}

impl<'w, T: Resource + Send + Sync> SystemParam for Option<ResMut<'w, T>> {
    type Item<'world> = Option<ResMut<'world, T>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        ResMut::<T>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.get_resource_mut::<T>())
    }
}

impl<'w, T: Resource> SystemParam for Option<NonSendRes<'w, T>> {
    type Item<'world> = Option<NonSendRes<'world, T>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        NonSendRes::<T>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(world.get_non_send_resource::<T>())
    }
}

impl<'w, T: Resource> SystemParam for Option<NonSendResMut<'w, T>> {
    type Item<'world> = Option<NonSendResMut<'world, T>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        NonSendResMut::<T>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        Ok(unsafe { world.get_non_send_resource_mut_unsafe::<T>() })
    }
}
