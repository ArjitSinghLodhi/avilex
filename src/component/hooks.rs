use rustc_hash::FxHashMap;
use std::{any::TypeId, marker::PhantomData, rc::Rc};

use crate::{
    commands::bundle::ComponentBundle,
    component::{ComponentMut, ComponentRef},
    ecs::component::Component,
    entity::Entity,
    events::{Event, EventReader, EventWriter},
    resources::{NonSend, NonSendMut, Res, ResMut, Resource},
    world::World,
};

pub struct ExclusiveWorld<'a> {
    pub(crate) world: &'a mut World,
}

impl<'a> ExclusiveWorld<'a> {
    pub fn get<'w, C: Component>(&'w self, entity: &'w Entity) -> Option<ComponentRef<'w, C>> {
        self.world.get::<C>(entity)
    }

    pub fn get_mut<'w, C: Component>(
        &'w mut self,
        entity: &'w Entity,
    ) -> Option<ComponentMut<'w, C>> {
        self.world.get_mut::<C>(entity)
    }

    pub fn spawn<B: ComponentBundle>(&mut self, component_bundle: B) -> Entity {
        self.world.spawn(component_bundle)
    }

    pub fn spawn_batch<B, I>(&mut self, components_iter: I) -> Vec<Entity>
    where
        B: ComponentBundle,
        I: Iterator<Item = B> + Send + 'static,
    {
        self.world.spawn_batch(components_iter)
    }

    pub fn insert_components<B: ComponentBundle>(&mut self, entity: Entity, component_bundle: B) {
        self.world.insert_components(entity, component_bundle);
    }

    pub fn remove_components<B: ComponentBundle>(&mut self, entity: Entity) {
        self.world.remove_components::<B>(entity);
    }

    pub fn despawn(&mut self, entity: Entity) {
        self.world.despawn(entity)
    }

    pub fn contains_resource<T: Resource + Send + Sync>(&self) -> bool {
        self.world.contains_resource::<T>()
    }

    pub fn insert_resource<T: Resource + Send + Sync>(&self, resource: T) -> Option<T> {
        self.world.insert_resource(resource)
    }

    pub fn remove_resource<T: Resource + Send + Sync>(&self) -> Option<T> {
        self.world.remove_resource::<T>()
    }

    pub fn resource<'w, R: Resource + Send + Sync>(&'w self) -> Res<'w, R> {
        self.world.resource::<R>()
    }

    pub fn resource_mut<'w, R: Resource + Send + Sync>(&'w self) -> ResMut<'w, R> {
        self.world.resource_mut::<R>()
    }

    pub fn get_resource<'w, R: Resource + Send + Sync>(&'w self) -> Option<Res<'w, R>> {
        self.world.get_resource::<R>()
    }

    pub fn get_resource_mut<'w, R: Resource + Send + Sync>(&'w self) -> Option<ResMut<'w, R>> {
        self.world.get_resource_mut::<R>()
    }

    pub fn contains_non_send_resource<T: Resource>(&self) -> bool {
        self.world.contains_non_send_resource::<T>()
    }

    pub fn insert_non_send_resource<T: Resource>(&mut self, resource: T) -> Option<T> {
        self.world.insert_non_send_resource(resource)
    }

    pub fn remove_non_send_resource<T: Resource>(&mut self) -> Option<T> {
        self.world.remove_non_send_resource::<T>()
    }

    pub fn non_send_resource<'w, R: Resource>(&'w self) -> NonSend<'w, R> {
        self.world.non_send_resource::<R>()
    }

    pub fn non_send_resource_mut<'w, R: Resource>(&'w mut self) -> NonSendMut<'w, R> {
        self.world.non_send_resource_mut::<R>()
    }

    pub fn get_non_send_resource<'w, R: Resource>(&'w self) -> Option<NonSend<'w, R>> {
        self.world.get_non_send_resource::<R>()
    }

    pub fn get_non_send_resource_mut<'w, R: Resource>(&'w mut self) -> Option<NonSendMut<'w, R>> {
        self.world.get_non_send_resource_mut::<R>()
    }

    /// Retrieve a mutable reference to a non-send resource using a shared reference.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSend`
    /// resource are active at the same time. Aliasing a mutable reference triggers
    /// immediate undefined behavior.
    pub unsafe fn non_send_resource_mut_unsafe<'w, R: Resource>(&'w self) -> NonSendMut<'w, R> {
        unsafe { self.world.non_send_resource_mut_unsafe::<R>() }
    }

    /// Safely attempt to retrieve a mutable reference to a non-send resource using a shared reference.
    /// Returns `None` if the resource does not exist.
    ///
    /// # Safety
    /// The caller must ensure that no other mutable references to the same `NonSend`
    /// resource are active at the same time.
    pub unsafe fn get_non_send_resource_mut_unsafe<'w, R: Resource>(
        &'w self,
    ) -> Option<NonSendMut<'w, R>> {
        unsafe { self.world.get_non_send_resource_mut_unsafe::<R>() }
    }

    pub fn event_writer<'w, E: Event>(&'w self) -> EventWriter<'w, E> {
        self.world.event_writer()
    }
    pub fn event_reader<'w, E: Event>(&'w self) -> EventReader<'w, E> {
        self.world.event_reader()
    }
}

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

    pub fn on_add(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_add.to_vec();
        current.push(hook);
        entry.on_add = Rc::from(current);
        self
    }

    pub fn on_insert(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_insert.to_vec();
        current.push(hook);
        entry.on_insert = Rc::from(current);
        self
    }

    pub fn on_discard(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_discard.to_vec();
        current.push(hook);
        entry.on_discard = Rc::from(current);
        self
    }

    pub fn on_remove(&mut self, hook: ComponentHook) -> &mut Self {
        let entry = self.registry.hooks.get_mut(&TypeId::of::<T>()).unwrap();
        let mut current = entry.on_remove.to_vec();
        current.push(hook);
        entry.on_remove = Rc::from(current);
        self
    }
}
