use rustc_hash::FxHashMap;
use std::{any::TypeId, marker::PhantomData, rc::Rc};

use crate::{
    commands::Commands,
    ecs::component::{Component, ComponentBundle},
    entity::{Entity, EntityMut, EntityRef},
    events::{Event, EventReader, EventWriter},
    resources::{NonSendRes, NonSendResMut, Res, ResMut, Resource},
    world::World,
};

/// Restricted exclusive world access passed into [`ComponentHook`] callbacks.
///
/// Exposes entity inspection, immediate structural operations, resource access, and event
/// dispatch while preventing disallowed lifecycle operations (such as registering new component
/// hooks mid-flush).
///
/// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
pub struct ExclusiveWorld<'w> {
    pub(crate) world: &'w mut World,
}

impl<'w> ExclusiveWorld<'w> {
    /// Returns a read-only [`EntityRef`] for inspecting components on `entity`.
    ///
    /// [`EntityRef`]: crate::ecs::entity::EntityRef
    pub fn entity<'a>(&'a self, entity: &'a Entity) -> EntityRef<'a> {
        self.world.entity(entity)
    }

    /// Returns a mutable [`EntityMut`] for inspecting and modifying components on `entity`.
    ///
    /// [`EntityMut`]: crate::ecs::entity::EntityMut
    pub fn entity_mut<'a>(&'a self, entity: &'a Entity) -> EntityMut<'a> {
        self.world.entity_mut(entity)
    }

    /// Spawns a new entity.
    ///
    /// Functions identically to [`Commands::spawn`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`Commands::spawn`]: crate::ecs::commands::Commands::spawn
    pub fn spawn<B: ComponentBundle>(&mut self, component_bundle: B) -> Entity {
        self.world.spawn(component_bundle)
    }

    /// Spawns a batch of entities from an iterator.
    ///
    /// Functions identically to [`Commands::spawn_batch`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`Commands::spawn_batch`]: crate::ecs::commands::Commands::spawn_batch
    pub fn spawn_batch<B, I>(&mut self, components_iter: I) -> Vec<Entity>
    where
        B: ComponentBundle,
        I: Iterator<Item = B> + Send + 'static,
    {
        self.world.spawn_batch(components_iter)
    }

    /// Inserts a bundle of components onto an existing entity.
    ///
    /// Functions identically to [`EntityCommands::insert`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::insert`]: crate::ecs::commands::EntityCommands::insert
    pub fn insert_components<B: ComponentBundle>(&mut self, entity: Entity, component_bundle: B) {
        self.world.insert_components(entity, component_bundle);
    }

    /// Removes a bundle of components from an entity.
    ///
    /// Functions identically to [`EntityCommands::remove`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::remove`]: crate::ecs::commands::EntityCommands::remove
    pub fn remove_components<B: ComponentBundle>(&mut self, entity: Entity) {
        self.world.remove_components::<B>(entity);
    }

    /// Despawns an entity.
    ///
    /// Functions identically to [`EntityCommands::despawn`], except structural changes take effect immediately.
    /// Refer to its documentation for more info.
    ///
    /// [`EntityCommands::despawn`]: crate::ecs::commands::EntityCommands::despawn
    pub fn despawn(&mut self, entity: Entity) {
        self.world.despawn(entity)
    }

    /// Returns a [`Commands`] buffer for queueing deferred structural changes to the world.
    ///
    /// [`Commands`]: crate::ecs::commands::Commands
    pub fn commands<'a>(&'a self) -> Commands<'a> {
        self.world.commands()
    }

    /// Returns `true` if a thread-safe [`Resource`] of type `T` exists in the world.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn contains_resource<T: Resource + Send + Sync>(&self) -> bool {
        self.world.contains_resource::<T>()
    }

    /// Inserts a thread-safe [`Resource`] of type `T` into the world, returning the previous value
    /// if one existed.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn insert_resource<T: Resource + Send + Sync>(&self, resource: T) -> Option<T> {
        self.world.insert_resource(resource)
    }

    /// Removes and returns the thread-safe [`Resource`] of type `T` from the world, if present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn remove_resource<T: Resource + Send + Sync>(&self) -> Option<T> {
        self.world.remove_resource::<T>()
    }

    /// Returns a shared [`Res`] guard to the thread-safe [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use [`ExclusiveWorld::get_resource`]
    /// for a non-panicking alternative.
    ///
    /// [`Res`]: crate::ecs::resources::Res
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_resource`]: Self::get_resource
    pub fn resource<'a, R: Resource + Send + Sync>(&'a self) -> Res<'a, R> {
        self.world.resource::<R>()
    }

    /// Returns an exclusive [`ResMut`] guard to the thread-safe [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_resource_mut`] for a non-panicking alternative.
    ///
    /// [`ResMut`]: crate::ecs::resources::ResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_resource_mut`]: Self::get_resource_mut
    pub fn resource_mut<'a, R: Resource + Send + Sync>(&'a self) -> ResMut<'a, R> {
        self.world.resource_mut::<R>()
    }

    /// Returns a shared [`Res`] guard to the thread-safe [`Resource`] of type `R`, or `None` if it
    /// does not exist.
    ///
    /// [`Res`]: crate::ecs::resources::Res
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_resource<'a, R: Resource + Send + Sync>(&'a self) -> Option<Res<'a, R>> {
        self.world.get_resource::<R>()
    }

    /// Returns an exclusive [`ResMut`] guard to the thread-safe [`Resource`] of type `R`, or `None`
    /// if it does not exist.
    ///
    /// [`ResMut`]: crate::ecs::resources::ResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_resource_mut<'a, R: Resource + Send + Sync>(&'a self) -> Option<ResMut<'a, R>> {
        self.world.get_resource_mut::<R>()
    }

    /// Returns `true` if a main-thread (`NonSend`) [`Resource`] of type `T` exists in the world.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn contains_non_send_resource<T: Resource>(&self) -> bool {
        self.world.contains_non_send_resource::<T>()
    }

    /// Inserts a main-thread (`NonSend`) [`Resource`] of type `T` into the world, returning the
    /// previous value if one existed.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn insert_non_send_resource<T: Resource>(&mut self, resource: T) -> Option<T> {
        self.world.insert_non_send_resource(resource)
    }

    /// Removes and returns the main-thread (`NonSend`) [`Resource`] of type `T` from the world,
    /// if present.
    ///
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn remove_non_send_resource<T: Resource>(&mut self) -> Option<T> {
        self.world.remove_non_send_resource::<T>()
    }

    /// Returns a shared [`NonSendRes`] reference to the main-thread [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_non_send_resource`] for a non-panicking alternative.
    ///
    /// [`NonSendRes`]: crate::ecs::resources::NonSendRes
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_non_send_resource`]: Self::get_non_send_resource
    pub fn non_send_resource<'a, R: Resource>(&'a self) -> NonSendRes<'a, R> {
        self.world.non_send_resource::<R>()
    }

    /// Returns an exclusive [`NonSendResMut`] reference to the main-thread [`Resource`] of type `R`.
    ///
    /// # Panics
    /// Panics if no resource of type `R` exists in the world. Use
    /// [`ExclusiveWorld::get_non_send_resource_mut`] for a non-panicking alternative.
    ///
    /// [`NonSendResMut`]: crate::ecs::resources::NonSendResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    /// [`ExclusiveWorld::get_non_send_resource_mut`]: Self::get_non_send_resource_mut
    pub fn non_send_resource_mut<'a, R: Resource>(&'a mut self) -> NonSendResMut<'a, R> {
        self.world.non_send_resource_mut::<R>()
    }

    /// Returns a shared [`NonSendRes`] reference to the main-thread [`Resource`] of type `R`, or
    /// `None` if it does not exist.
    ///
    /// [`NonSendRes`]: crate::ecs::resources::NonSendRes
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_non_send_resource<'a, R: Resource>(&'a self) -> Option<NonSendRes<'a, R>> {
        self.world.get_non_send_resource::<R>()
    }

    /// Returns an exclusive [`NonSendResMut`] reference to the main-thread [`Resource`] of type
    /// `R`, or `None` if it does not exist.
    ///
    /// [`NonSendResMut`]: crate::ecs::resources::NonSendResMut
    /// [`Resource`]: crate::ecs::resources::Resource
    pub fn get_non_send_resource_mut<'a, R: Resource>(
        &'a mut self,
    ) -> Option<NonSendResMut<'a, R>> {
        self.world.get_non_send_resource_mut::<R>()
    }

    /// Retrieve a mutable reference to a non-send resource using a shared reference.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSendRes`
    /// resource are active at the same time. Aliasing a mutable reference triggers
    /// immediate undefined behavior.
    pub unsafe fn non_send_resource_mut_unsafe<'a, R: Resource>(&'a self) -> NonSendResMut<'a, R> {
        unsafe { self.world.non_send_resource_mut_unsafe::<R>() }
    }

    /// Safely attempt to retrieve a mutable reference to a non-send resource using a shared reference.
    /// Returns `None` if the resource does not exist.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSendRes`
    /// resource are active at the same time.
    pub unsafe fn get_non_send_resource_mut_unsafe<'a, R: Resource>(
        &'a self,
    ) -> Option<NonSendResMut<'a, R>> {
        unsafe { self.world.get_non_send_resource_mut_unsafe::<R>() }
    }

    /// Returns an [`EventWriter`] for dispatching events of type `E`.
    ///
    /// [`EventWriter`]: crate::ecs::events::EventWriter
    pub fn event_writer<'a, E: Event>(&'a self) -> EventWriter<'a, E> {
        self.world.event_writer()
    }

    /// Returns an [`EventReader`] for reading events of type `E`.
    ///
    /// [`EventReader`]: crate::ecs::events::EventReader
    pub fn event_reader<'a, E: Event>(&'a self) -> EventReader<'a, E> {
        self.world.event_reader()
    }
}

/// Function pointer type executed during [`Component`] lifecycle events (`on_add`, `on_insert`,
/// `on_discard`, or `on_remove`).
///
/// [`Component`]: crate::ecs::component::Component
pub type ComponentHook = fn(&mut ExclusiveWorld, &Entity);

#[derive(Clone, Default)]
pub(crate) struct ComponentHooks {
    pub(crate) on_add: Rc<[ComponentHook]>,
    pub(crate) on_insert: Rc<[ComponentHook]>,
    pub(crate) on_remove: Rc<[ComponentHook]>,
    pub(crate) on_discard: Rc<[ComponentHook]>,
}

pub(crate) struct ComponentHooksRegistry {
    pub(crate) hooks: FxHashMap<TypeId, ComponentHooks>,
    auto_registered: FxHashMap<TypeId, bool>,
}

impl ComponentHooksRegistry {
    pub(crate) fn new() -> Self {
        Self {
            hooks: FxHashMap::default(),
            auto_registered: FxHashMap::default(),
        }
    }

    pub(crate) fn prepopulate_hooks<T: Component>(&mut self) {
        let type_id = TypeId::of::<T>();

        if self.hooks.contains_key(&type_id) {
            return;
        }

        self.hooks.insert(type_id, ComponentHooks::default());
        self.auto_registered.insert(type_id, false);
    }

    pub(crate) fn register_hooks<'a, T: Component>(
        &'a mut self,
    ) -> ComponentHooksRegisterer<'a, T> {
        self.prepopulate_hooks::<T>();
        ComponentHooksRegisterer::new(self)
    }

    pub(crate) fn get_hooks_or_init<C: Component>(&mut self) -> ComponentHooks {
        let type_id = TypeId::of::<C>();

        let is_auto = self.auto_registered.get(&type_id).copied().unwrap_or(false);
        if is_auto && let Some(hooks) = self.hooks.get(&type_id) {
            return hooks.clone();
        }

        let entry = self.hooks.entry(type_id).or_default();

        let mut on_add_vec = entry.on_add.to_vec();
        let mut on_insert_vec = entry.on_insert.to_vec();
        let mut on_remove_vec = entry.on_remove.to_vec();
        let mut on_discard_vec = entry.on_discard.to_vec();

        if let Some(h) = C::on_add() {
            on_add_vec.push(h);
        }
        if let Some(h) = C::on_insert() {
            on_insert_vec.push(h);
        }
        if let Some(h) = C::on_remove() {
            on_remove_vec.push(h);
        }
        if let Some(h) = C::on_discard() {
            on_discard_vec.push(h);
        }

        entry.on_add = Rc::from(on_add_vec);
        entry.on_insert = Rc::from(on_insert_vec);
        entry.on_remove = Rc::from(on_remove_vec);
        entry.on_discard = Rc::from(on_discard_vec);

        self.auto_registered.insert(type_id, true);
        entry.clone()
    }

    pub(crate) fn get_hooks(&self, type_id: TypeId) -> Option<ComponentHooks> {
        self.hooks.get(&type_id).cloned()
    }

    pub(crate) fn is_auto_registered(&self, type_id: TypeId) -> bool {
        self.auto_registered.get(&type_id).copied().unwrap_or(false)
    }
}

/// Builder returned by [`World::register_component_hooks`] for registering
/// manual lifecycle [`ComponentHook`] callbacks on component `T`.
///
/// Manual hooks are appended after any trait-level hooks defined on [`Component`], preserving
/// baseline trait behavior.
///
/// [`World::register_component_hooks`]: crate::ecs::world::World::register_component_hooks
/// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
/// [`Component`]: crate::ecs::component::Component
pub struct ComponentHooksRegisterer<'a, T: Component> {
    registry: &'a mut ComponentHooksRegistry,
    _marker: PhantomData<T>,
}

impl<'a, T: Component> ComponentHooksRegisterer<'a, T> {
    pub(crate) fn new(registry: &'a mut ComponentHooksRegistry) -> Self {
        Self {
            registry,
            _marker: PhantomData,
        }
    }

    /// Appends a [`ComponentHook`] executed when component `T` is newly added to an entity.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_add(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_add.to_vec();
        current.push(hook);
        entry.on_add = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed whenever component `T` is inserted or overwritten on
    /// an entity.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_insert(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_insert.to_vec();
        current.push(hook);
        entry.on_insert = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed right before an existing instance of component `T` is
    /// overwritten.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_discard(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_discard.to_vec();
        current.push(hook);
        entry.on_discard = Rc::from(current);
        self
    }

    /// Appends a [`ComponentHook`] executed when component `T` is removed from an entity or
    /// dropped during despawn.
    ///
    /// [`ComponentHook`]: crate::ecs::component::hooks::ComponentHook
    pub fn on_remove(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_remove.to_vec();
        current.push(hook);
        entry.on_remove = Rc::from(current);
        self
    }
}
